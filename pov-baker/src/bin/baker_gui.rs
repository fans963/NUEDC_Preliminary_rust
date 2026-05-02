//! # POV Baker GUI — Bevy 3D 预览 + egui 控制面板
//!
//! 功能：
//! - 3D 视窗：GPU 渲染带材质光照的 3D 模型，自动旋转
//! - 16×16 LED 矩阵实时预览：CPU 光栅化当前角度的降采样结果
//! - 参数调节：形状类型、尺寸、渲染配置
//! - 烘焙导出：生成 postcard 动画并保存为 .bin 文件
//! - 上传到 ESP32：HTTP POST 到设备

use bevy::prelude::*;
use bevy::window::WindowResolution;
use bevy_egui::{EguiContexts, EguiPlugin, EguiPrimaryContextPass, egui};
use egui::{FontData, FontDefinitions, FontFamily};
use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use rfd::FileDialog;
use pov_baker::bevy::{
    BevyRenderer, SceneSource, Tri,
    get_triangles, render_angle_from_tris,
};
use pov_baker::RenderConfig;
use pov_core::Frame;
use std::f32::consts::TAU;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "POV Baker — 3D Preview".to_string(),
                resolution: WindowResolution::new(1280, 800).with_scale_factor_override(1.0),
                // Wayland (niri) 优化 — 强制前台焦点和输入捕获
                present_mode: bevy::window::PresentMode::Fifo,
                focused: true,
                ..default()
            }),
            ..default()
        }))
        .add_plugins(EguiPlugin::default())
        .init_resource::<GuiState>()
        .init_resource::<BakeResult>()
        .init_resource::<WindowFocusState>()
        .add_systems(Startup, setup_scene)
        .add_systems(Update, (
            ensure_window_focus,
            update_wayland_input,
            orbit_camera,
            update_preview,
            handle_shape_change,
        ))
        .add_systems(EguiPrimaryContextPass, gui_panel)
        .run();
}

// ── 资源定义 ────────────────────────────────────────────────

/// GUI 状态 — 用户可调参数
#[derive(Resource)]
struct GuiState {
    shape: ShapeKind,
    half_size: f32,
    radius: f32,
    height: f32,
    num_steps: usize,
    render_size: u32,
    ortho_size: f32,
    camera_distance: f32,
    orbit_speed: f32,
    orbit_angle: f32,
    auto_orbit: bool,
    /// 当前相机角度对应的 16×16 帧
    current_frame: Frame,
    /// 三角形缓存（形状变化时重新生成）
    triangles: Vec<Tri>,
    /// 形状是否发生变化（需要重建 mesh）
    shape_dirty: bool,
    /// ESP32 IP 地址
    esp_ip: String,
    /// 状态消息
    status_msg: String,
    /// 模型文件路径
    model_path: String,
    /// 已加载的模型源
    model_source: Option<SceneSource>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ShapeKind {
    Cube,
    Sphere,
    Cylinder,
    Cone,
    Model,
}

impl std::fmt::Display for ShapeKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ShapeKind::Cube => write!(f, "正方体"),
            ShapeKind::Sphere => write!(f, "球体"),
            ShapeKind::Cylinder => write!(f, "圆柱体"),
            ShapeKind::Cone => write!(f, "圆锥体"),
            ShapeKind::Model => write!(f, "模型文件"),
        }
    }
}
impl Default for GuiState {
    fn default() -> Self {
        let source = SceneSource::ProceduralCube { half_size_mm: 14.0 };
        let triangles = get_triangles(&source).unwrap_or_default();
        Self {
            shape: ShapeKind::Cube,
            half_size: 14.0,
            radius: 12.0,
            height: 24.0,
            num_steps: 1024,
            render_size: 1024,
            ortho_size: 6.0,
            camera_distance: 10.0,
            orbit_speed: 0.3,
            orbit_angle: 0.0,
            auto_orbit: true,
            current_frame: [0u16; 16],
            triangles,
            shape_dirty: false,
            esp_ip: "192.168.4.1".to_string(),
            status_msg: "就绪".to_string(),
            model_path: String::new(),
            model_source: None,
        }
    }
}

impl Default for WindowFocusState {
    fn default() -> Self {
        Self {
            last_focus_request: std::time::Instant::now(),
        }
    }
}

/// 烘焙结果
#[derive(Resource, Default)]
struct BakeResult {
    postcard_bytes: Option<Vec<u8>>,
}

/// 标记 3D 场景中的预览 mesh
#[derive(Component)]
struct PreviewMesh;

/// 标记轨道相机
#[derive(Component)]
struct OrbitCamera;

/// Wayland 焦点管理
#[derive(Resource)]
struct WindowFocusState {
    last_focus_request: std::time::Instant,
}

// ── 场景初始化 ──────────────────────────────────────────────

fn setup_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // 地面网格参考
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default())),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgba(0.15, 0.15, 0.2, 0.3),
            alpha_mode: AlphaMode::Blend,
            ..default()
        })),
        Transform::from_translation(Vec3::new(0.0, -3.5, 0.0)).with_scale(Vec3::splat(20.0)),
    ));

    // 预览模型
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::from_size(Vec3::splat(2.8)))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.2, 0.7, 1.0),
            metallic: 0.3,
            perceptual_roughness: 0.4,
            ..default()
        })),
        PreviewMesh,
    ));

    // 线框包围盒 — 16×16 矩阵区域（6cm = 6.0 场景单位宽）
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::from_size(Vec3::splat(6.0)))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgba(1.0, 1.0, 0.0, 0.08),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            ..default()
        })),
    ));

    // 灯光
    commands.spawn((
        DirectionalLight {
            illuminance: 8000.0,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_xyz(5.0, 8.0, 5.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        PointLight {
            intensity: 200_000.0,
            range: 30.0,
            ..default()
        },
        Transform::from_xyz(-5.0, 5.0, -5.0),
    ));

    // 环境光
    commands.insert_resource(GlobalAmbientLight {
        color: Color::srgb(0.6, 0.65, 0.8).into(),
        brightness: 300.0,
        ..default()
    });

    // 轨道相机
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 5.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y),
        OrbitCamera,
    ));
}

fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();

    // 尝试在 Linux 系统中寻找中文字体
    let font_paths = [
        "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/adobe-source-han-sans/SourceHanSansCN-Regular.otf",
        "/usr/share/fonts/TTF/DroidSansFallback.ttf",
    ];

    let mut font_loaded = false;
    for path in font_paths {
        if let Ok(font_data) = std::fs::read(path) {
            fonts.font_data.insert(
                "my_font".to_owned(),
                FontData::from_owned(font_data).into(),
            );
            fonts.families.get_mut(&FontFamily::Proportional).unwrap()
                .insert(0, "my_font".to_owned());
            fonts.families.get_mut(&FontFamily::Monospace).unwrap()
                .push("my_font".to_owned());
            font_loaded = true;
            break;
        }
    }

    if font_loaded {
        ctx.set_fonts(fonts);
    }
}

// ── Wayland 焦点管理 ────────────────────────────────────────

fn ensure_window_focus(
    mut windows: Query<&mut Window>,
    mut focus_state: ResMut<WindowFocusState>,
) {
    // 每 100ms 检查并确保窗口焦点
    let now = std::time::Instant::now();
    if now.duration_since(focus_state.last_focus_request).as_millis() > 100 {
        for mut window in windows.iter_mut() {
            if !window.focused {
                window.focused = true;
            }
        }
        focus_state.last_focus_request = now;
    }
}

fn update_wayland_input(
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    windows: Query<&Window>,
    mut state: ResMut<GuiState>,
) {
    // 诊断：检测输入设备激活
    let has_mouse_input = mouse.get_pressed().next().is_some();
    let has_key_input = keys.get_pressed().next().is_some();
    let any_input = has_mouse_input || has_key_input;

    if any_input {
        for window in windows.iter() {
            if let Some(_pos) = window.cursor_position() {
                if state.status_msg.is_empty() || state.status_msg.contains("Wayland") {
                    state.status_msg = "✅ Wayland 输入检测到".to_string();
                }
            }
        }
    }
}

// ── 相机轨道 ────────────────────────────────────────────────

fn orbit_camera(
    time: Res<Time>,
    mut state: ResMut<GuiState>,
    mut query: Query<&mut Transform, With<OrbitCamera>>,
) {
    if state.auto_orbit {
        state.orbit_angle += state.orbit_speed * time.delta_secs();
        if state.orbit_angle > TAU {
            state.orbit_angle -= TAU;
        }
    }
    let theta = state.orbit_angle;
    let dist = state.camera_distance;
    for mut tf in query.iter_mut() {
        tf.translation = Vec3::new(
            dist * theta.sin(),
            dist * 0.5,
            dist * theta.cos(),
        );
        tf.look_at(Vec3::ZERO, Vec3::Y);
    }
}

// ── 实时 CPU 预览更新 ───────────────────────────────────────

fn update_preview(mut state: ResMut<GuiState>) {
    if state.triangles.is_empty() {
        state.current_frame = [0u16; 16];
        return;
    }
    let theta = state.orbit_angle;
    let frame = render_angle_from_tris(
        &state.triangles,
        theta,
        state.render_size,
        state.ortho_size,
        state.camera_distance,
    );
    state.current_frame = frame;
}

// ── 形状变更处理 ────────────────────────────────────────────

fn handle_shape_change(
    mut state: ResMut<GuiState>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut query: Query<&mut Mesh3d, With<PreviewMesh>>,
) {
    if !state.shape_dirty {
        return;
    }
    state.shape_dirty = false;

    let source = match state.shape {
        ShapeKind::Cube => SceneSource::ProceduralCube { half_size_mm: state.half_size },
        ShapeKind::Sphere => SceneSource::ProceduralSphere { radius_mm: state.radius },
        ShapeKind::Cylinder => SceneSource::ProceduralCylinder {
            radius_mm: state.radius,
            height_mm: state.height,
        },
        ShapeKind::Cone => SceneSource::ProceduralCone {
            radius_mm: state.radius,
            height_mm: state.height,
        },
        ShapeKind::Model => {
            let Some(ref source) = state.model_source else {
                state.triangles.clear();
                state.status_msg = "❌ 未加载模型".to_string();
                return;
            };
            source.clone()
        }
    };

    // 更新 CPU 三角形缓存
    state.triangles = get_triangles(&source).unwrap_or_default();

    // 更新 GPU mesh
    let gpu_mesh = match state.shape {
        ShapeKind::Cube => {
            let hs = state.half_size / 10.0;
            Mesh::from(Cuboid::from_size(Vec3::splat(hs * 2.0)))
        }
        ShapeKind::Sphere => Mesh::from(Sphere { radius: state.radius / 10.0 }),
        ShapeKind::Cylinder => Mesh::from(Cylinder {
            radius: state.radius / 10.0,
            half_height: state.height / 10.0 / 2.0,
        }),
        ShapeKind::Cone => Mesh::from(Cone {
            radius: state.radius / 10.0,
            height: state.height / 10.0,
        }),
        ShapeKind::Model => mesh_from_triangles(&state.triangles),
    };

    let handle = meshes.add(gpu_mesh);
    for mut mesh3d in query.iter_mut() {
        mesh3d.0 = handle.clone();
    }

    state.status_msg = format!("形状已更新: {} ({} 三角形)",
        state.shape, state.triangles.len());
}

// ── egui 面板 ───────────────────────────────────────────────

fn gui_panel(
    mut contexts: EguiContexts,
    mut state: ResMut<GuiState>,
    mut bake_result: ResMut<BakeResult>,
    mut fonts_ready: Local<bool>,
) -> bevy::ecs::error::Result {
    let ctx = contexts.ctx_mut()?;

    if !*fonts_ready {
        setup_fonts(ctx);
        *fonts_ready = true;
    }

    // 调试：检查 egui 是否捕获到了点击
    if ctx.input(|i| i.pointer.any_click()) {
        info!("🔴 Egui 检测到点击动作！位置: {:?}", ctx.input(|i| i.pointer.interact_pos()));
    }

    egui::Window::new("控制面板 (POV Baker)")
        .default_pos([640.0, 400.0])
        .default_size([600.0, 750.0])
        .show(ctx, |ui| {
            ui.heading("🎯 POV Baker");
            ui.separator();

            // ── 形状选择 ────────────────────────────────
            ui.label("形状类型");
            let prev_shape = state.shape;
            egui::ComboBox::from_id_salt("shape_selector")
                .selected_text(format!("{}", state.shape))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut state.shape, ShapeKind::Cube, "正方体");
                    ui.selectable_value(&mut state.shape, ShapeKind::Sphere, "球体");
                    ui.selectable_value(&mut state.shape, ShapeKind::Cylinder, "圆柱体");
                    ui.selectable_value(&mut state.shape, ShapeKind::Cone, "圆锥体");
                    ui.selectable_value(&mut state.shape, ShapeKind::Model, "模型文件");
                });
            if state.shape != prev_shape {
                state.shape_dirty = true;
            }

            ui.separator();

            // ── 参数滑块 ────────────────────────────────
            if state.shape == ShapeKind::Model {
                ui.label("模型文件路径");
                ui.horizontal(|ui| {
                    ui.text_edit_singleline(&mut state.model_path);
                    if ui.button("选择文件").clicked() {
                        if let Some(path) = FileDialog::new()
                            .add_filter("3D Models", &["gltf", "glb", "obj", "stl"])
                            .pick_file()
                        {
                            state.model_path = path.to_string_lossy().to_string();
                            match model_source_from_path(&state.model_path) {
                                Ok(source) => {
                                    state.model_source = Some(source);
                                    state.shape_dirty = true;
                                    state.status_msg = "✅ 模型已加载".to_string();
                                }
                                Err(err) => {
                                    state.status_msg = format!("❌ 模型路径无效: {}", err);
                                }
                            }
                        }
                    }
                });
                if ui.button("加载模型").clicked() {
                    match model_source_from_path(&state.model_path) {
                        Ok(source) => {
                            state.model_source = Some(source);
                            state.shape_dirty = true;
                            state.status_msg = "✅ 模型已加载".to_string();
                        }
                        Err(err) => {
                            state.status_msg = format!("❌ 模型路径无效: {}", err);
                        }
                    }
                }
            } else {
                ui.label("📐 形状参数 (mm)");
                let mut changed = false;
                match state.shape {
                    ShapeKind::Cube => {
                        changed |= ui.add(egui::Slider::new(&mut state.half_size, 5.0..=30.0)
                            .text("半边长")).changed();
                    }
                    ShapeKind::Sphere => {
                        changed |= ui.add(egui::Slider::new(&mut state.radius, 3.0..=30.0)
                            .text("半径")).changed();
                    }
                    ShapeKind::Cylinder | ShapeKind::Cone => {
                        changed |= ui.add(egui::Slider::new(&mut state.radius, 3.0..=30.0)
                            .text("半径")).changed();
                        changed |= ui.add(egui::Slider::new(&mut state.height, 5.0..=50.0)
                            .text("高度")).changed();
                    }
                    ShapeKind::Model => {}
                }
                if changed {
                    state.shape_dirty = true;
                }
            }

            ui.separator();

            // ── 渲染配置 ────────────────────────────────
            ui.collapsing("⚙️ 渲染配置", |ui| {
                ui.add(egui::Slider::new(&mut state.num_steps, 64..=2048)
                    .text("角度步数")
                    .logarithmic(true));
                ui.add(egui::Slider::new(&mut state.ortho_size, 2.0..=20.0)
                    .text("正交尺寸"));
                if ui.add(egui::Slider::new(&mut state.camera_distance, 5.0..=30.0)
                    .text("相机距离")).changed() {
                    // camera_distance 变化不需要重建 mesh，只影响渲染
                }
            });

            ui.separator();

            // ── 相机控制 ────────────────────────────────
            ui.horizontal(|ui| {
                ui.checkbox(&mut state.auto_orbit, "自动旋转");
                ui.add(egui::Slider::new(&mut state.orbit_speed, 0.05..=2.0)
                    .text("转速"));
            });
            if !state.auto_orbit {
                ui.add(egui::Slider::new(&mut state.orbit_angle, 0.0..=TAU)
                    .text("角度"));
            }
            let angle_deg = state.orbit_angle.to_degrees();
            ui.label(format!("当前角度: {:.1}°", angle_deg));

            ui.separator();

            // ── 16×16 LED 预览 ──────────────────────────
            ui.heading("💡 LED 矩阵预览");
            draw_led_grid(ui, &state.current_frame);

            ui.separator();

            // ── 烘焙 & 导出 ─────────────────────────────
            ui.heading("📦 导出");

            if ui.button("🔥 烘焙动画").clicked() {
                if state.shape == ShapeKind::Model && state.model_source.is_none() {
                    state.status_msg = "❌ 请先加载模型文件".to_string();
                    return;
                }
                let source = current_scene_source(&state);
                let config = RenderConfig {
                    num_steps: state.num_steps,
                    render_size: state.render_size,
                    ortho_size: state.ortho_size,
                    camera_distance: state.camera_distance,
                };
                let renderer = BevyRenderer::new(config);
                match renderer.render_to_postcard(&source) {
                    Ok(bytes) => {
                        state.status_msg = format!(
                            "✅ 烘焙完成: {} bytes ({} 步)",
                            bytes.len(), state.num_steps
                        );
                        bake_result.postcard_bytes = Some(bytes);
                    }
                    Err(e) => {
                        state.status_msg = format!("❌ 烘焙失败: {:?}", e);
                    }
                }
            }

            let has_bake = bake_result.postcard_bytes.is_some();
            ui.add_enabled_ui(has_bake, |ui| {
                if ui.button("💾 保存 anim.bin").clicked() {
                    if let Some(ref bytes) = bake_result.postcard_bytes {
                        match std::fs::write("anim.bin", bytes) {
                            Ok(_) => state.status_msg = format!(
                                "✅ 已保存 anim.bin ({} bytes)", bytes.len()
                            ),
                            Err(e) => state.status_msg = format!("❌ 保存失败: {}", e),
                        }
                    }
                }
            });

            ui.separator();

            // ── 上传到 ESP32 ────────────────────────────
            ui.heading("📡 上传到 ESP32");
            ui.horizontal(|ui| {
                ui.label("IP:");
                ui.text_edit_singleline(&mut state.esp_ip);
            });
            ui.add_enabled_ui(has_bake, |ui| {
                if ui.button("🚀 上传").clicked() {
                    if let Some(ref bytes) = bake_result.postcard_bytes {
                        let url = format!("http://{}/api/upload", state.esp_ip);
                        state.status_msg = format!("⏳ 正在上传到 {} ...", url);
                        match upload_to_esp(&url, bytes) {
                            Ok(resp) => state.status_msg = format!("✅ 上传成功: {}", resp),
                            Err(e) => state.status_msg = format!("❌ 上传失败: {}", e),
                        }
                    }
                }
            });

            ui.separator();

            // ── 状态栏 ──────────────────────────────────
            ui.colored_label(
                if state.status_msg.starts_with('✅') {
                    egui::Color32::from_rgb(100, 255, 100)
                } else if state.status_msg.starts_with('❌') {
                    egui::Color32::from_rgb(255, 100, 100)
                } else {
                    egui::Color32::from_rgb(200, 200, 200)
                },
                &state.status_msg,
            );
        });

    Ok(())
}

// ── 16×16 LED 网格绘制 ──────────────────────────────────────

fn draw_led_grid(ui: &mut egui::Ui, frame: &Frame) {
    let cell_size = 16.0;
    let spacing = 2.0;
    let grid_size = cell_size * 16.0 + spacing * 15.0;

    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(grid_size, grid_size),
        egui::Sense::hover(),
    );

    let painter = ui.painter_at(rect);

    // 背景
    painter.rect_filled(rect, 4.0, egui::Color32::from_rgb(10, 10, 15));

    // 统计亮灯数
    let mut lit_count = 0u32;

    for row in 0..16 {
        let row_bits = frame[row];
        for col in 0..16 {
            let lit = (row_bits >> col) & 1 == 1;
            if lit {
                lit_count += 1;
            }

            let x = rect.min.x + col as f32 * (cell_size + spacing);
            let y = rect.min.y + row as f32 * (cell_size + spacing);
            let cell_rect = egui::Rect::from_min_size(
                egui::pos2(x, y),
                egui::vec2(cell_size, cell_size),
            );

            let color = if lit {
                egui::Color32::from_rgb(0, 255, 80)
            } else {
                egui::Color32::from_rgb(20, 25, 20)
            };
            painter.rect_filled(cell_rect, 2.0, color);
        }
    }

    ui.label(format!("亮灯: {}/256", lit_count));
}

fn mesh_from_triangles(triangles: &[Tri]) -> Mesh {
    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(triangles.len() * 3);
    let mut normals: Vec<[f32; 3]> = Vec::with_capacity(triangles.len() * 3);
    let mut indices: Vec<u32> = Vec::with_capacity(triangles.len() * 3);

    for (i, tri) in triangles.iter().enumerate() {
        let base = (i * 3) as u32;
        positions.push(tri.v0);
        positions.push(tri.v1);
        positions.push(tri.v2);

        let v0 = Vec3::from(tri.v0);
        let v1 = Vec3::from(tri.v1);
        let v2 = Vec3::from(tri.v2);
        let mut n = (v1 - v0).cross(v2 - v0);
        if n.length_squared() > 0.0 {
            n = n.normalize();
        } else {
            n = Vec3::Z;
        }
        let normal = [n.x, n.y, n.z];
        normals.push(normal);
        normals.push(normal);
        normals.push(normal);

        indices.push(base);
        indices.push(base + 1);
        indices.push(base + 2);
    }

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

fn model_source_from_path(path: &str) -> Result<SceneSource, String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("路径为空".to_string());
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower.ends_with(".gltf") || lower.ends_with(".glb") {
        Ok(SceneSource::GltfFile(trimmed.to_string()))
    } else if lower.ends_with(".obj") {
        Ok(SceneSource::ObjFile(trimmed.to_string()))
    } else if lower.ends_with(".stl") {
        Ok(SceneSource::StlFile(trimmed.to_string()))
    } else {
        Err("仅支持 .gltf/.glb/.obj/.stl".to_string())
    }
}

// ── 辅助函数 ────────────────────────────────────────────────

fn current_scene_source(state: &GuiState) -> SceneSource {
    match state.shape {
        ShapeKind::Cube => SceneSource::ProceduralCube { half_size_mm: state.half_size },
        ShapeKind::Sphere => SceneSource::ProceduralSphere { radius_mm: state.radius },
        ShapeKind::Cylinder => SceneSource::ProceduralCylinder {
            radius_mm: state.radius,
            height_mm: state.height,
        },
        ShapeKind::Cone => SceneSource::ProceduralCone {
            radius_mm: state.radius,
            height_mm: state.height,
        },
        ShapeKind::Model => state.model_source.clone().unwrap_or_else(|| {
            SceneSource::ProceduralCube { half_size_mm: 14.0 }
        }),
    }
}

fn upload_to_esp(url: &str, data: &[u8]) -> Result<String, String> {
    let mut resp = ureq::post(url)
        .header("Content-Type", "application/octet-stream")
        .send(data)
        .map_err(|e| format!("{}", e))?;
    resp.body_mut()
        .read_to_string()
        .map_err(|e| format!("读取响应失败: {}", e))
}
