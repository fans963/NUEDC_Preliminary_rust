// Bevy — 仅用 Bevy 的几何原语生成形状 + gltf 实体加载
// 无材质、无光照、无 GPU。纯 CPU 正交投影 + 软件光栅化。
extern crate std;

use alloc::vec::Vec;
use bevy::mesh::{Mesh, VertexAttributeValues};
use bevy::prelude::{Cone, Cuboid, Cylinder, Sphere, Vec3};
use pov_core::serial::{Animation, Keyframe, Page};
use pov_core::Frame;
use std::collections::HashMap;

use crate::pipeline::downsample_to_frame;
use crate::renderer::{FrameRenderer, RenderConfig, RenderResult};

#[derive(Debug, Clone)]
pub enum SceneSource {
    GltfFile(String),
    ProceduralCube { half_size_mm: f32 },
    ProceduralSphere { radius_mm: f32 },
    ProceduralCylinder { radius_mm: f32, height_mm: f32 },
    ProceduralCone { radius_mm: f32, height_mm: f32 },
}

#[derive(Debug)]
pub enum BevyRenderError {
    AssetLoadFailed(String),
    NoGeometry,
    Postcard,
}

pub struct BevyRenderer {
    pub config: RenderConfig,
}

impl BevyRenderer {
    pub fn new(config: RenderConfig) -> Self {
        Self { config }
    }

    pub fn render_to_postcard(&self, source: &SceneSource) -> Result<Vec<u8>, BevyRenderError> {
        let triangles = get_triangles(source)?;

        let size = self.config.render_size;
        let ortho = self.config.ortho_size;
        let dist = self.config.camera_distance;
        let num_steps = self.config.num_steps;

        let mut keyframes = Vec::new();
        let mut prev: Frame = [0u16; 16];

        for step in 0..num_steps {
            let theta = step as f32 / num_steps as f32 * core::f32::consts::TAU;
            let frame = render_angle_from_tris(&triangles, theta, size, ortho, dist);
            if frame != prev {
                keyframes.push(Keyframe {
                    angle_tick: step as u16,
                    frame,
                });
                prev = frame;
            }
        }

        let anim = Animation {
            pages: alloc::vec![Page { keyframes }],
        };
        postcard::to_allocvec(&anim).map_err(|_| BevyRenderError::Postcard)
    }
}

/// 获取场景源对应的三角形列表。
pub fn get_triangles(source: &SceneSource) -> Result<Vec<Tri>, BevyRenderError> {
    let triangles = match source {
        SceneSource::GltfFile(path) => load_gltf_triangles(path)?,
        _ => procedural_triangles(source),
    };
    if triangles.is_empty() {
        return Err(BevyRenderError::NoGeometry);
    }
    Ok(triangles)
}

/// 从三角形列表渲染单个角度的 16×16 帧（CPU 软件光栅化）。
pub fn render_angle_from_tris(
    triangles: &[Tri],
    theta: f32,
    render_size: u32,
    ortho_size: f32,
    camera_distance: f32,
) -> Frame {
    let mvp = build_mvp(
        camera_distance * libm::sinf(theta),
        camera_distance * libm::cosf(theta),
        ortho_size,
        render_size,
    );

    let mut color_buf = vec![0u8; (render_size * render_size * 4) as usize];
    let mut depth_buf = vec![-1.0f32; (render_size * render_size) as usize];

    for tri in triangles {
        rasterize(&mvp, tri, &mut color_buf, &mut depth_buf, render_size);
    }

    downsample_to_frame(&color_buf, render_size, render_size, 128)
}

// ── 三角形提取 ──────────────────────────────────────────────────────────────

fn extract_triangles(mesh: &Mesh) -> Vec<Tri> {
    let mut triangles = Vec::new();
    let Some(positions) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else {
        return triangles;
    };
    let VertexAttributeValues::Float32x3(verts) = positions else {
        return triangles;
    };
    let indices: Vec<u32> = match mesh.indices() {
        Some(indices) => indices.iter().map(|i| i as u32).collect(),
        None => (0..verts.len() as u32).collect(),
    };
    for chunk in indices.chunks(3) {
        if chunk.len() < 3 {
            continue;
        }
        triangles.push(Tri {
            v0: verts[chunk[0] as usize],
            v1: verts[chunk[1] as usize],
            v2: verts[chunk[2] as usize],
        });
    }
    triangles
}

/// 程序化几何体 — 直接用 Bevy 的 Mesh::from(primitive)
fn procedural_triangles(source: &SceneSource) -> Vec<Tri> {
    let mesh = match source {
        SceneSource::ProceduralCube { half_size_mm } => {
            let hs = half_size_mm / 10.0;
            Mesh::from(Cuboid::from_size(Vec3::splat(hs * 2.0)))
        }
        SceneSource::ProceduralSphere { radius_mm } => {
            Mesh::from(Sphere { radius: radius_mm / 10.0 })
        }
        SceneSource::ProceduralCylinder { radius_mm, height_mm } => {
            Mesh::from(Cylinder {
                radius: radius_mm / 10.0,
                half_height: height_mm / 10.0 / 2.0,
            })
        }
        SceneSource::ProceduralCone { radius_mm, height_mm } => {
            Mesh::from(Cone {
                radius: radius_mm / 10.0,
                height: height_mm / 10.0,
            })
        }
        _ => return Vec::new(),
    };
    extract_triangles(&mesh)
}

/// glTF 加载 — 用 gltf crate（专门的文件加载器）
fn load_gltf_triangles(path: &str) -> Result<Vec<Tri>, BevyRenderError> {
    use std::collections::HashMap;
    use std::path::Path;

    let path = Path::new(path);
    let gltf = gltf::Gltf::open(path)
        .map_err(|e| BevyRenderError::AssetLoadFailed(format!("{}", e)))?;

    let bin_data = gltf.blob.clone().unwrap_or_default();
    let mut buffers = Vec::new();
    for buf in gltf.buffers() {
        match buf.source() {
            gltf::buffer::Source::Bin => buffers.push(bin_data.clone()),
            gltf::buffer::Source::Uri(uri) => {
                let data = std::fs::read(path.parent().unwrap().join(uri))
                    .map_err(|e| BevyRenderError::AssetLoadFailed(format!("{}", e)))?;
                buffers.push(data);
            }
        }
    }

    // 收集 mesh 世界变换
    let mut mesh_transforms: HashMap<usize, Vec<[[f32; 4]; 4]>> = HashMap::new();
    for scene in gltf.scenes() {
        collect_transforms(scene.nodes(), &IDENTITY, &mut mesh_transforms);
    }

    let mut triangles = Vec::new();

    for mesh in gltf.meshes() {
        let transforms = mesh_transforms.remove(&mesh.index()).unwrap_or_default();

        for primitive in mesh.primitives() {
            let reader = primitive.reader(|buffer| Some(&buffers[buffer.index()]));
            let Some(pos_iter) = reader.read_positions() else { continue };
            let positions: Vec<[f32; 3]> = pos_iter.collect();
            let indices: Vec<u32> = match reader.read_indices() {
                Some(iter) => iter.into_u32().collect(),
                None => (0..positions.len() as u32).collect(),
            };

            for chunk in indices.chunks(3) {
                if chunk.len() < 3 {
                    continue;
                }
                let p = [
                    positions[chunk[0] as usize],
                    positions[chunk[1] as usize],
                    positions[chunk[2] as usize],
                ];
                if transforms.is_empty() {
                    triangles.push(Tri { v0: p[0], v1: p[1], v2: p[2] });
                } else {
                    for m in &transforms {
                        triangles.push(Tri {
                            v0: transform_point(m, &p[0]),
                            v1: transform_point(m, &p[1]),
                            v2: transform_point(m, &p[2]),
                        });
                    }
                }
            }
        }
    }

    Ok(triangles)
}

const IDENTITY: [[f32; 4]; 4] = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];

fn collect_transforms<'a>(
    nodes: impl Iterator<Item = gltf::Node<'a>>,
    parent: &[[f32; 4]; 4],
    out: &mut HashMap<usize, Vec<[[f32; 4]; 4]>>,
) {
    for node in nodes {
        let local = node.transform().matrix();
        let world = mat4_mul(parent, &local);
        if let Some(mesh) = node.mesh() {
            out.entry(mesh.index()).or_default().push(world);
        }
        collect_transforms(node.children(), &world, out);
    }
}

fn mat4_mul(a: &[[f32; 4]; 4], b: &[[f32; 4]; 4]) -> [[f32; 4]; 4] {
    let mut r = [[0.0f32; 4]; 4];
    for i in 0..4 {
        for j in 0..4 {
            for k in 0..4 {
                r[j][i] += a[k][i] * b[j][k];
            }
        }
    }
    r
}

fn transform_point(m: &[[f32; 4]; 4], p: &[f32; 3]) -> [f32; 3] {
    let x = m[0][0] * p[0] + m[1][0] * p[1] + m[2][0] * p[2] + m[3][0];
    let y = m[0][1] * p[0] + m[1][1] * p[1] + m[2][1] * p[2] + m[3][1];
    let z = m[0][2] * p[0] + m[1][2] * p[1] + m[2][2] * p[2] + m[3][2];
    let w = m[0][3] * p[0] + m[1][3] * p[1] + m[2][3] * p[2] + m[3][3];
    if w != 0.0 {
        [x / w, y / w, z / w]
    } else {
        [x, y, z]
    }
}

// ── 正交投影 + 光栅化 ──────────────────────────────────────────────────────

pub struct Tri {
    pub v0: [f32; 3],
    pub v1: [f32; 3],
    pub v2: [f32; 3],
}

fn build_mvp(cx: f32, cz: f32, ortho_size: f32, render_size: u32) -> [f32; 16] {
    let flen = libm::sqrtf(cx * cx + cz * cz);
    let fx = -cx / flen;
    let fz = -cz / flen;

    let rx = -fz;
    let rz = fx;
    let rlen = libm::sqrtf(rx * rx + rz * rz);
    let rx = rx / rlen;
    let rz = rz / rlen;

    let uy = rz * fx - rx * fz;
    let eye = [cx, 0.0, cz];

    let view: [f32; 16] = [
        rx, 0.0, -fx, 0.0, //
        0.0, uy, 0.0, 0.0, //
        rz, 0.0, -fz, 0.0, //
        -(rx * eye[0] + rz * eye[2]),
        -(uy * eye[1]),
        fx * eye[0] + fz * eye[2],
        1.0,
    ];

    let proj: [f32; 16] = [
        2.0 / (ortho_size * 2.0), 0.0, 0.0, 0.0, //
        0.0, 2.0 / (ortho_size * 2.0), 0.0, 0.0, //
        0.0, 0.0, -2.0 / 200.0, 0.0, //
        0.0, 0.0, 0.0, 1.0, //
    ];

    let half = render_size as f32 / 2.0;
    let vp: [f32; 16] = [
        half, 0.0, 0.0, 0.0, //
        0.0, -half, 0.0, 0.0, //
        0.0, 0.0, 1.0, 0.0, //
        half, half, 0.0, 1.0, //
    ];

    mat4x_mul(&vp, &mat4x_mul(&proj, &view))
}

fn mat4x_mul(a: &[f32; 16], b: &[f32; 16]) -> [f32; 16] {
    let mut r = [0.0f32; 16];
    for i in 0..4 {
        for j in 0..4 {
            for k in 0..4 {
                r[j * 4 + i] += a[k * 4 + i] * b[j * 4 + k];
            }
        }
    }
    r
}

fn transform_screen(mvp: &[f32; 16], p: &[f32; 3]) -> [f32; 3] {
    let x = mvp[0] * p[0] + mvp[4] * p[1] + mvp[8] * p[2] + mvp[12];
    let y = mvp[1] * p[0] + mvp[5] * p[1] + mvp[9] * p[2] + mvp[13];
    let z = mvp[2] * p[0] + mvp[6] * p[1] + mvp[10] * p[2] + mvp[14];
    let w = mvp[3] * p[0] + mvp[7] * p[1] + mvp[11] * p[2] + mvp[15];
    if w != 0.0 {
        [x / w, y / w, z / w]
    } else {
        [x, y, z]
    }
}

fn rasterize(mvp: &[f32; 16], tri: &Tri, color: &mut [u8], depth: &mut [f32], size: u32) {
    let sv0 = transform_screen(mvp, &tri.v0);
    let sv1 = transform_screen(mvp, &tri.v1);
    let sv2 = transform_screen(mvp, &tri.v2);

    let area = (sv1[0] - sv0[0]) * (sv2[1] - sv0[1])
        - (sv2[0] - sv0[0]) * (sv1[1] - sv0[1]);
    if area <= 0.0 { return; }
    let area_recip = 1.0 / area;

    let min_x = sv0[0].min(sv1[0]).min(sv2[0]).max(0.0) as u32;
    let max_x = sv0[0].max(sv1[0]).max(sv2[0]).min((size - 1) as f32) as u32;
    let min_y = sv0[1].min(sv1[1]).min(sv2[1]).max(0.0) as u32;
    let max_y = sv0[1].max(sv1[1]).max(sv2[1]).min((size - 1) as f32) as u32;

    for y in min_y..=max_y {
        let py = y as f32 + 0.5;
        for x in min_x..=max_x {
            let px = x as f32 + 0.5;
            let w0 = ((sv1[0] - px) * (sv2[1] - py) - (sv1[1] - py) * (sv2[0] - px)) * area_recip;
            let w1 = ((sv2[0] - px) * (sv0[1] - py) - (sv2[1] - py) * (sv0[0] - px)) * area_recip;
            let w2 = 1.0 - w0 - w1;
            if w0 >= 0.0 && w1 >= 0.0 && w2 >= 0.0 {
                let z = w0 * sv0[2] + w1 * sv1[2] + w2 * sv2[2];
                let idx = (y * size + x) as usize;
                if z > depth[idx] {
                    depth[idx] = z;
                    let ci = idx * 4;
                    color[ci] = 255;
                    color[ci + 1] = 255;
                    color[ci + 2] = 255;
                    color[ci + 3] = 255;
                }
            }
        }
    }
}

// ── FrameRenderer trait ────────────────────────────────────────────────────

impl FrameRenderer for BevyRenderer {
    type Error = BevyRenderError;
    fn render_file(&self, _path: &str) -> Result<RenderResult, Self::Error> {
        Err(BevyRenderError::NoGeometry)
    }
    fn render_bytes(&self, _bytes: &[u8]) -> Result<RenderResult, Self::Error> {
        Err(BevyRenderError::NoGeometry)
    }
}
