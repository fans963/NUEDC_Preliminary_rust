//! # POV 系统状态机
//!
//! 使用 `statig` 分层状态机管理 POV 系统生命周期。
//!
//! ## 设计
//!
//! ```text
//!         ┌──────────┐
//!         │  Idle    │──Start──▶ Running
//!         │          │◀─Stop────│
//!         └──────────┘          │
//!              │                │
//!         Upload│           Upload│
//!              ▼                ▼
//!         ┌─────────────────────┐
//!         │     Loading         │
//!         │  (停旋转 + 扩容)     │
//!         └──────────┬──────────┘
//!                    │ 扩容完成 → 自动 Start → Running
//!                    │
//!              Fault │          Fault
//!                    ▼
//!              ┌─────────┐
//!              │  Error  │──Reset──▶ Idle
//!              └─────────┘
//! ```
//!
//! `Idle`/`Running` 收到 `Upload` 后进入 `Loading` 状态。
//! display_task 检测到 `Loading` 后暂停显示轮询，
//! upload handler 完成 postcard 扩容后发 `Start` 回到 `Running`。

use core::cell::RefCell;
use statig::blocking::{IntoStateMachineExt, StateMachine};
use statig::prelude::*;

// ── Events ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    Start,
    Stop,
    Upload,
    Fault,
    Reset,
}

// ── Shared data ───────────────────────────────────────────────────────────

#[derive(Default)]
pub struct PovSystem;

// ── State machine ─────────────────────────────────────────────────────────

#[state_machine(
    initial = "State::idle()",
    state(derive(Debug, Clone, Copy, PartialEq, Eq)),
    superstate(derive(Debug, Clone, Copy, PartialEq, Eq)),
)]
impl PovSystem {
    /// 待机：电机停转，不显示
    #[state(superstate = "ready")]
    fn idle(&mut self, event: &Event) -> Outcome<State> {
        match event {
            Event::Start => Transition(State::running()),
            Event::Upload => Transition(State::loading()),
            Event::Fault => Transition(State::error()),
            _ => Super,
        }
    }

    /// 运行：电机旋转，显示轮询
    #[state(superstate = "ready")]
    fn running(&mut self, event: &Event) -> Outcome<State> {
        match event {
            Event::Stop => Transition(State::idle()),
            Event::Upload => Transition(State::loading()),
            Event::Fault => Transition(State::error()),
            _ => Super,
        }
    }

    /// 上传中：暂停旋转，CPU 专注处理扩容
    #[state]
    fn loading(&mut self, event: &Event) -> Outcome<State> {
        match event {
            Event::Start => Transition(State::running()),
            Event::Fault => Transition(State::error()),
            _ => Super,
        }
    }

    #[superstate]
    fn ready(&mut self, event: &Event) -> Outcome<State> {
        let _ = event;
        Super
    }

    /// 故障：可 Reset 恢复
    #[state]
    fn error(&mut self, event: &Event) -> Outcome<State> {
        match event {
            Event::Reset => Transition(State::idle()),
            Event::Upload => Handled,
            _ => Super,
        }
    }
}

// ── 单核零开销包装器 ──────────────────────────────────────────

/// 状态机包装器。`RefCell` 运行时借用检查，无锁。
pub struct PovState {
    inner: RefCell<StateMachine<PovSystem>>,
}

impl PovState {
    pub fn new() -> Self {
        Self {
            inner: RefCell::new(PovSystem::default().state_machine()),
        }
    }

    /// 发送事件给状态机。同步，零阻塞。
    pub fn handle(&self, event: &Event) {
        self.inner.borrow_mut().handle(event);
    }

    /// 获取当前状态名。
    pub fn state_name(&self) -> &'static str {
        let sm = self.inner.borrow();
        let s = sm.state();
        if *s == State::idle()        { "idle" }
        else if *s == State::running()  { "running" }
        else if *s == State::loading()  { "loading" }
        else if *s == State::error()    { "error" }
        else                            { "unknown" }
    }

    /// 是否正在运行（显示轮询激活）
    pub fn is_running(&self) -> bool {
        self.state_name() == "running"
    }

    /// 是否正在上传/扩容（显示应暂停）
    pub fn is_loading(&self) -> bool {
        self.state_name() == "loading"
    }
}

impl Default for PovState {
    fn default() -> Self {
        Self::new()
    }
}
