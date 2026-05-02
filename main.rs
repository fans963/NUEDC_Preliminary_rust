use bevy::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_systems(Startup, setup)
        // 注册 Update 系统的旋转和坐标系绘制
        .add_systems(Update, (draw_axes, rotate_dna))
        .run();
}

// 标记 DNA 根节点的组件，用于旋转
#[derive(Component)]
struct DnaRoot;

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let backbone_radius = 0.15;
    let rung_radius = 0.04;
    let r = 1.2;          // 螺旋的半径
    let h = 0.4;          // 每弧度的上升高度
    let steps = 60;       // 分段数
    let max_t = std::f32::consts::PI * 6.0; // 旋转 3 圈
    let dt = max_t / steps as f32;

    // 复用网格
    let sphere_mesh = meshes.add(Sphere::new(backbone_radius));
    
    // 第一条骨架的材质（发光蓝色）
    let mat_b1 = materials.add(StandardMaterial {
        base_color: Color::srgb(0.1, 0.5, 1.0),
        emissive: LinearRgba::rgb(0.2, 1.0, 2.0).into(),
        ..default()
    });
    // 第二条骨架的材质（发光紫色）
    let mat_b2 = materials.add(StandardMaterial {
        base_color: Color::srgb(0.8, 0.1, 1.0),
        emissive: LinearRgba::rgb(2.0, 0.2, 2.0).into(),
        ..default()
    });

    // 碱基对的四种颜色 (A, T, C, G)
    let rung_mats = [
        materials.add(StandardMaterial { base_color: Color::srgb(1.0, 0.2, 0.2), ..default() }), // 红
        materials.add(StandardMaterial { base_color: Color::srgb(0.2, 1.0, 0.2), ..default() }), // 绿
        materials.add(StandardMaterial { base_color: Color::srgb(1.0, 0.8, 0.2), ..default() }), // 黄
        materials.add(StandardMaterial { base_color: Color::srgb(0.2, 0.8, 1.0), ..default() }), // 青
    ];

    // 创建一个父节点，将整个 DNA 挂载在上面，方便统一旋转
    commands.spawn((
        Transform::default(),
        Visibility::default(),
        DnaRoot,
    )).with_children(|parent| {
        for i in 0..=steps {
            let t = i as f32 * dt;
            let y = t * h - (max_t * h / 2.0); // 居中于 y=0
            
            // 骨架1的位置
            let p1 = Vec3::new(r * t.cos(), y, r * t.sin());
            // 骨架2的位置（相差 180 度即 PI）
            let p2 = Vec3::new(r * (t + std::f32::consts::PI).cos(), y, r * (t + std::f32::consts::PI).sin());

            // 绘制骨架1上的球体
            parent.spawn((
                Mesh3d(sphere_mesh.clone()),
                MeshMaterial3d(mat_b1.clone()),
                Transform::from_translation(p1),
            ));

            // 绘制骨架2上的球体
            parent.spawn((
                Mesh3d(sphere_mesh.clone()),
                MeshMaterial3d(mat_b2.clone()),
                Transform::from_translation(p2),
            ));

            // 连接两个球体的圆柱体（碱基对）
            let dist = p1.distance(p2);
            let rung_mesh = meshes.add(Cylinder::new(rung_radius, dist));
            let mid = (p1 + p2) * 0.5;

            parent.spawn((
                Mesh3d(rung_mesh),
                MeshMaterial3d(rung_mats[i % 4].clone()),
                Transform::from_translation(mid)
                    // 使圆柱体的 Y 轴对齐从 p1 到 p2 的方向
                    .with_rotation(Quat::from_rotation_arc(Vec3::Y, (p2 - p1).normalize())),
            ));
        }
    });

    // 添加一些漂亮的光源
    commands.spawn((
        PointLight {
            intensity: 8_000_000.0,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_xyz(4.0, 8.0, 4.0),
    ));
    commands.spawn((
        PointLight {
            intensity: 4_000_000.0,
            color: Color::srgb(0.8, 0.8, 1.0),
            ..default()
        },
        Transform::from_xyz(-4.0, -2.0, -4.0),
    ));

    // 摄像机稍微拉远一点，能看到全貌
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 2.0, 8.0).looking_at(Vec3::ZERO, Dir3::Y),
    ));
}

// 旋转动画：每秒绕 Y 轴旋转
fn rotate_dna(time: Res<Time>, mut query: Query<&mut Transform, With<DnaRoot>>) {
    for mut transform in &mut query {
        transform.rotate_y(time.delta_secs() * 0.5);
    }
}

// 依然保留坐标系
fn draw_axes(mut gizmos: Gizmos) {
    gizmos.axes(Transform::IDENTITY, 2.0);
}
