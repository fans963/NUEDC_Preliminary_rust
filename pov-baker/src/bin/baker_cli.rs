//! POV Baker — 统一 CLI (Bevy)
//!
//! 所有形状经 Bevy 渲染管道。
use clap::{Parser, Subcommand};
use pov_baker::bevy::{BevyRenderer, SceneSource};
use pov_baker::RenderConfig;

#[derive(Parser)]
#[command(name = "pov-baker", version, about = "3D → POV 帧烘焙工具 (Bevy)")]
struct Cli {
    #[command(subcommand)]
    command: Command,
    #[arg(short, long, default_value = "anim.bin", global = true)]
    output: String,
    #[arg(short, long, default_value_t = 1024, global = true)]
    steps: usize,
}

#[derive(Subcommand)]
enum Command {
    Cube {
        #[arg(long, default_value_t = 14.0)]
        half_size: f32,
    },
    Sphere {
        #[arg(long, default_value_t = 12.0)]
        radius: f32,
    },
    Cylinder {
        #[arg(long, default_value_t = 12.0)]
        radius: f32,
        #[arg(long, default_value_t = 24.0)]
        height: f32,
    },
    Cone {
        #[arg(long, default_value_t = 12.0)]
        radius: f32,
        #[arg(long, default_value_t = 24.0)]
        height: f32,
    },
    Gltf {
        #[arg(short, long)]
        input: String,
    },
    Mock,
}

fn bake(source: SceneSource, steps: usize) -> Vec<u8> {
    let config = match &source {
        SceneSource::GltfFile(_) => RenderConfig::default(),
        _ => RenderConfig {
            num_steps: steps, render_size: 256,
            ortho_size: 8.0, camera_distance: 10.0,
        },
    };
    BevyRenderer::new(config)
        .render_to_postcard(&source)
        .unwrap_or_else(|e| {
            eprintln!("Bevy 渲染失败: {:?}", e);
            std::process::exit(1);
        })
}

fn main() {
    let cli = Cli::parse();
    let postcard = match &cli.command {
        Command::Cube { half_size } => {
            eprintln!("正方体: {}mm (Bevy)", half_size * 2.0);
            bake(SceneSource::ProceduralCube { half_size_mm: *half_size }, cli.steps)
        }
        Command::Sphere { radius } => {
            eprintln!("球体: radius={}mm (Bevy)", radius);
            bake(SceneSource::ProceduralSphere { radius_mm: *radius }, cli.steps)
        }
        Command::Cylinder { radius, height } => {
            eprintln!("圆柱体: r={}mm h={}mm (Bevy)", radius, height);
            bake(SceneSource::ProceduralCylinder { radius_mm: *radius, height_mm: *height }, cli.steps)
        }
        Command::Cone { radius, height } => {
            eprintln!("圆锥体: r={}mm h={}mm (Bevy)", radius, height);
            bake(SceneSource::ProceduralCone { radius_mm: *radius, height_mm: *height }, cli.steps)
        }
        Command::Gltf { input } => {
            eprintln!("glTF: {} (Bevy)", input);
            let renderer = BevyRenderer::new(RenderConfig {
                num_steps: cli.steps, ..Default::default()
            });
            renderer.render_to_postcard(&SceneSource::GltfFile(input.clone()))
                .unwrap_or_else(|e| { eprintln!("Bevy 失败: {:?}", e); std::process::exit(1); })
        }
        Command::Mock => {
            eprintln!("Mock (Bevy)");
            bake(SceneSource::ProceduralCube { half_size_mm: 14.0 }, cli.steps)
        }
    };
    eprintln!("输出: {} 字节", postcard.len());
    std::fs::write(&cli.output, &postcard).unwrap_or_else(|e| {
        eprintln!("写入失败: {}", e); std::process::exit(1);
    });
    eprintln!("已写入: {}", cli.output);
}
