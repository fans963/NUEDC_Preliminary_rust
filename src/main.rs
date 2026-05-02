#![no_std]
#![no_main]

extern crate alloc;

use alloc::vec::Vec;
use defmt::info;
use embassy_executor::Spawner;
use embassy_net::{Config, Stack, StackResources};
use embassy_time::{Duration, Ticker, Timer};
use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::rng::Rng;
use esp_hal::timer::timg::TimerGroup;
use esp_radio::wifi::{Config as WifiConfig, ControllerConfig, sta::StationConfig};
use picoserve::routing::{get, post};
use pov_firmware::display::driver::{NullWriter, PovDisplay};
use pov_firmware::display::pool;
use pov_firmware::fsm::{Event, PovState};
use pov_firmware::web::{api::ApiRouter, api::ApiRoute};
use static_cell::StaticCell;

esp_bootloader_esp_idf::esp_app_desc!();

const WIFI_SSID: &str = "快船总冠军";
const WIFI_PASSWORD: &str = "12345678910a";

static STACK: StaticCell<Stack<'static>> = StaticCell::new();
static POV_STATE: StaticCell<PovState> = StaticCell::new();

const LUT_STEPS: usize = 1024;

// ── Display polling task ────────────────────────────────────────────────────

/// 显示轮询任务：30kHz Ticker → 读编码器角度 → 查表 → DMA 输出。
///
/// # 上传暂停机制
///
/// 单核下显示和 WiFi 共享 CPU。上传时 FSM 进入 Loading 状态，
/// display_task 检测到后暂停显示，把 CPU 让给 upload handler 做扩容。
/// 完成后 FSM 回到 Running，自动恢复 30kHz 轮询。
///
/// 首次使用前，将 `NullWriter` 替换为实际的 `FrameWriter` 实现。
#[embassy_executor::task]
async fn display_task(mut display: PovDisplay<NullWriter>, state: &'static PovState) {
    info!("Display poll task started (30 kHz)");

    let mut ticker = Ticker::every(Duration::from_micros(33));
    let mut prev_lut_idx: usize = 0;

    loop {
        // ── 检查 FSM 状态：Loading 时暂停显示 ─────────────────────
        if state.is_loading() {
            display.clear();
            Timer::after(Duration::from_millis(50)).await;
            continue;
        }

        // ── 正常轮询路径 ───────────────────────────────────────────
        // 轮询编码器，获取 LUT 索引
        let lut_idx = 0; // ← 替换为如下 DMA SPI 读取：
        //
        // 初始化（在 main() 中做一次）：
        //   let spi = Spi::new(peripherals.SPI2, 1_000_000u32.Hz(), SpiMode::Mode3)?
        //       .with_sck(peripherals.GPIO6)
        //       .with_mosi(peripherals.GPIO7)
        //       .with_miso(peripherals.GPIO2)
        //       .with_dma(peripherals.DMA_CH0);         // ← DMA 传输
        //   let cs = Output::new(peripherals.GPIO10, Level::High);
        //   let dev = ExclusiveDevice::new(spi, cs)?;
        //   let mut encoder = As5047p::new(dev);
        //
        // 轮询循环内调用（DMA 内部阻塞等待，但传输由硬件完成）：
        //   let lut_idx = encoder.read_lut_index(LUT_STEPS)?;

        // 检测转数回绕：当前角度 < 上次角度 且 上次是后半圈 → 一圈结束
        if lut_idx < prev_lut_idx && prev_lut_idx > LUT_STEPS / 2 {
            pool::increment_rev();
        }
        prev_lut_idx = lut_idx;

        // 渲染当前角度对应的帧
        display.render_angle(lut_idx);

        ticker.next().await;
    }
}

// ── Background tasks ────────────────────────────────────────────────────────

#[embassy_executor::task]
async fn web_dashboard_task(state: &'static PovState) {
    loop {
        let name = state.state_name();
        let anim_count = pool::animation_count();
        info!(
            "[Dashboard] state={}, animations={}",
            name, anim_count
        );
        Timer::after(Duration::from_millis(1000)).await;
    }
}

#[embassy_executor::task]
async fn net_stack_task(
    mut runner: embassy_net::Runner<'static, esp_radio::wifi::Interface<'static>>,
) {
    runner.run().await;
}

#[embassy_executor::task]
async fn http_server_task(stack: Stack<'static>, state: &'static PovState) {
    let mut buffer = [0u8; 2048];

    let config = picoserve::Config::new(picoserve::Timeouts::default());

    let router = picoserve::Router::new()
        .route(
            "/",
            get(|| async {
                (
                    ("Content-Type", "text/html; charset=utf-8"),
                    pov_firmware::web::http::WebService::get_index_html(),
                )
            }),
        )
        .route(
            "/api/status",
            get(move || async move { ApiRouter::handle_get(&ApiRoute::Status, state) }),
        )
        .route(
            "/api/start",
            post(move || async move {
                ApiRouter::handle_post(&ApiRoute::Start, state, None)
            }),
        )
        .route(
            "/api/stop",
            post(move || async move {
                ApiRouter::handle_post(&ApiRoute::Stop, state, None)
            }),
        )
        .route(
            "/api/upload",
            post(move |body: Vec<u8>| async move {
                // 1. 状态机 → Loading（display_task 检测后自动暂停）
                state.handle(&Event::Upload);
                // 2. Yield 让 display_task 看到 Loading 状态
                Timer::after(Duration::from_millis(1)).await;
                // 3. 解析并加载动画
                let resp = ApiRouter::handle_post(&ApiRoute::Upload, state, Some(&body));
                // 4. 恢复：状态机 → Running（display_task 自动恢复轮询）
                state.handle(&Event::Start);
                resp
            }),
        );

    info!("HTTP server task started");

    let mut tcp_rx_buffer = [0u8; 4096];
    let mut tcp_tx_buffer = [0u8; 4096];

    let server = picoserve::Server::new(&router, &config, &mut buffer);

    server
        .listen_and_serve("HTTP", stack, 80, &mut tcp_rx_buffer, &mut tcp_tx_buffer)
        .await;
}

// ── 入口 ────────────────────────────────────────────────────────────────────

#[allow(clippy::large_stack_frames)]
#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    rtt_target::rtt_init_defmt!();
    
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    // PSRAM 作为堆内存（8 MB），所有 alloc::Vec / String 自动走 PSRAM。
    // KEYFRAME_STORE 等热路径静态数组仍在内部 DRAM。
    esp_alloc::psram_allocator!(peripherals.PSRAM, esp_hal::psram);

    // ── 电机 PWM (LEDC) ───────────────────────────────────────────
    // 使用 LEDC 的定时器 0 + 通道 0, 输出到 IO3:
    //
    //   let mut ledc = Ledc::new(peripherals.LEDC);
    //   let mut timer = ledc.timer::<HighSpeed>(&mut ledc.timer0);
    //   timer.configure(TimerConfig {
    //       duty: Duty::Duty14Bit,
    //       clock_source: ClockSource::PllClk,
    //       frequency: 50u32.kHz(),
    //   });
    //   let mut channel = ledc.channel(channel0, peripherals.GPIO3);
    //   channel.configure(&mut timer, &mut channel, &mut timer)?;
    //
    // PID 输出（speed_loop.rs 的 update_from_raw 返回值）→ ledc 占空比：
    //   channel.set_duty(raw_duty)?;

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let sw_interrupt =
        esp_hal::interrupt::software::SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);

    esp_rtos::start(timg0.timer0, sw_interrupt.software_interrupt0);

    let station_config = WifiConfig::Station(
        StationConfig::default()
            .with_ssid(WIFI_SSID)
            .with_password(WIFI_PASSWORD.into()),
    );
    let controller_config = ControllerConfig::default().with_initial_config(station_config);
    let (mut wifi_controller, interfaces) =
        esp_radio::wifi::new(peripherals.WIFI, controller_config)
            .expect("failed to initialize WiFi controller");

    let seed = Rng::new().random() as u64;

    let (stack, runner) = embassy_net::new(
        interfaces.station,
        Config::dhcpv4(Default::default()),
        {
            static RESOURCES: StaticCell<StackResources<3>> = StaticCell::new();
            RESOURCES.init(StackResources::new())
        },
        seed,
    );

    let stack = &*STACK.init(stack);

    spawner.spawn(net_stack_task(runner).unwrap());

    match wifi_controller.connect_async().await {
        Ok(info) => {
            info!(
                "WiFi connected: ssid={}, channel={}, aid={}",
                info.ssid.as_str(),
                info.channel,
                info.aid
            );
        }
        Err(err) => {
            info!("WiFi connection failed: {:?}", err);
        }
    }

    info!("Waiting for IP address...");
    loop {
        if let Some(config) = stack.config_v4() {
            info!("Got IP: {}", config.address);
            break;
        }
        Timer::after(Duration::from_millis(500)).await;
    }

    info!("Embassy initialized!");

    // ── 加载固化动画 ─────────────────────────────────────────────────────
    pov_firmware::display::embedded::load_embedded();
    info!("Embedded cube animation loaded");

    let pov_state = POV_STATE.init(PovState::new());

    // ── 启动显示轮询任务 ─────────────────────────────────────────────────
    // 在实际硬件上，这里应创建编码器 SPI + 显示 SPI，然后：
    //
    //   let encoder = As5047p::new(encoder_spi);
    //   let display_driver = PovDisplay::new(display_writer, LUT_STEPS);
    //   spawner.spawn(display_task(display_driver).unwrap());
    //
    // 当前使用空 writer 占位
    let null_writer = pov_firmware::display::driver::NullWriter;
    let display_driver = PovDisplay::new(null_writer, LUT_STEPS);
    spawner.spawn(display_task(display_driver, pov_state).unwrap());

    spawner.spawn(web_dashboard_task(pov_state).unwrap());

    if let Ok(token) = http_server_task(*stack, pov_state) {
        spawner.spawn(token);
    }

    loop {
        info!("POV system running");
        Timer::after(Duration::from_secs(5)).await;
    }
}
