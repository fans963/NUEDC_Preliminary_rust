/// build.rs — 编译时生成固化 POV 动画 + ESP32 链接器设置
use std::path::Path;

fn main() {
    linker_be_nice();
    println!("cargo:rustc-link-arg=-Tdefmt.x");
    println!("cargo:rustc-link-arg=-Tlinkall.x");

    println!("cargo:rerun-if-changed=build.rs");

    // 用 Bevy 生成固化正方体动画
    let renderer = pov_baker::bevy::BevyRenderer::new(pov_baker::RenderConfig {
        num_steps: 1024,
        render_size: 256,
        ortho_size: 6.0,
        camera_distance: 10.0,
    });
    let postcard = renderer
        .render_to_postcard(&pov_baker::bevy::SceneSource::ProceduralCube {
            half_size_mm: 14.0,
        })
        .expect("build.rs: Bevy cube generation failed");

    let out_dir = std::env::var("OUT_DIR").unwrap();
    let out_path = Path::new(&out_dir).join("embedded_anim.postcard");
    std::fs::write(&out_path, &postcard).unwrap();

    // 估算原始帧大小（1024 * 32 = 32768）
    println!(
        "cargo:warning=Embedded animation: 1024 frames → {} bytes postcard (saved {} KB vs raw LUT)",
        postcard.len(),
        (32768 - postcard.len()) / 1024,
    );
}

fn linker_be_nice() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        let kind = &args[1];
        let what = &args[2];
        match kind.as_str() {
            "undefined-symbol" => match what.as_str() {
                w if w.starts_with("_defmt_") => {
                    eprintln!();
                    eprintln!("💡 `defmt` not found - make sure `defmt.x` is added as a linker script and you have included `use defmt_rtt as _;`");
                    eprintln!();
                }
                "_stack_start" => {
                    eprintln!();
                    eprintln!("💡 Is the linker script `linkall.x` missing?");
                    eprintln!();
                }
                w if w.starts_with("esp_rtos_") => {
                    eprintln!();
                    eprintln!("💡 `esp-radio` has no scheduler enabled. Make sure you have initialized `esp-rtos` or provided an external scheduler.");
                    eprintln!();
                }
                _ => (),
            },
            _ => std::process::exit(1),
        }
        std::process::exit(0);
    }
    println!(
        "cargo:rustc-link-arg=--error-handling-script={}",
        std::env::current_exe().unwrap().display()
    );
}
