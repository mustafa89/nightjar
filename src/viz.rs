//! The stage visualizer: a WGSL fragment shader painted into the stage's `El::effect` slot from
//! Rune's compositor hook, fed every frame with the analyser's bins, waveform, bass/mid/treble
//! and beat, and the skin's palette. Four modes share one pipeline.

use crate::analysis::{BINS, WAVE};
use crate::skin::{rgb, skin};
use crate::state::{Play, State};
use rune::compose::{Compositor, DeviceKey, Frame};
use std::collections::HashMap;

pub const VIZ_MARK: u32 = 1;

const SHADER: &str = r#"
struct U {
    rect: vec4f,   // slot in pass pixels: x, y, w, h
    view: vec4f,   // pass w, h, time, mode
    lv: vec4f,     // bass, mid, treble, beat
    misc: vec4f,   // encode to sRGB, playing, corner radius px, energy
    c0: vec4f,
    c1: vec4f,
    c2: vec4f,
    bins: array<vec4f, 16>,
    wave: array<vec4f, 16>,
};
@group(0) @binding(0) var<uniform> u: U;

struct V { @builtin(position) pos: vec4f };

@vertex
fn vs(@builtin(vertex_index) vi: u32) -> V {
    var corners = array<vec2f, 6>(vec2f(0.0, 0.0), vec2f(1.0, 0.0), vec2f(0.0, 1.0), vec2f(0.0, 1.0), vec2f(1.0, 0.0), vec2f(1.0, 1.0));
    let px = u.rect.xy + corners[vi] * u.rect.zw;
    let ndc = px / u.view.xy * 2.0 - 1.0;
    var o: V;
    o.pos = vec4f(ndc.x, -ndc.y, 0.0, 1.0);
    return o;
}

const TAU: f32 = 6.2831853;

fn bin(x: f32) -> f32 {
    let f = clamp(x, 0.0, 1.0) * 63.0;
    let i = u32(floor(f));
    let j = min(i + 1u, 63u);
    let a = u.bins[i / 4u][i % 4u];
    let b = u.bins[j / 4u][j % 4u];
    return mix(a, b, fract(f));
}

fn wave(x: f32) -> f32 {
    let f = clamp(x, 0.0, 1.0) * 63.0;
    let i = u32(floor(f));
    let j = min(i + 1u, 63u);
    let a = u.wave[i / 4u][i % 4u];
    let b = u.wave[j / 4u][j % 4u];
    return mix(a, b, smoothstep(0.0, 1.0, fract(f)));
}

// The skin's three colours round a loop.
fn palette(t: f32) -> vec3f {
    let x = fract(t) * 3.0;
    if x < 1.0 { return mix(u.c0.rgb, u.c1.rgb, smoothstep(0.0, 1.0, x)); }
    if x < 2.0 { return mix(u.c1.rgb, u.c2.rgb, smoothstep(0.0, 1.0, x - 1.0)); }
    return mix(u.c2.rgb, u.c0.rgb, smoothstep(0.0, 1.0, x - 2.0));
}

fn hash(p: vec2f) -> f32 {
    return fract(sin(dot(p, vec2f(127.1, 311.7))) * 43758.5453);
}

fn aurora(q: vec2f, t: f32) -> vec3f {
    let r = length(q);
    let a = atan2(q.y, q.x) / TAU + 0.5;
    let z = 0.32 / (r + 0.04) + t * 1.2;
    let s = bin(abs(a * 2.0 - 1.0));
    let stripes = pow(0.5 + 0.5 * sin(a * TAU * 6.0 + z * 1.7 + sin(z * 0.7) * 2.0), 3.0);
    let ring = pow(0.5 + 0.5 * cos(z * TAU * 0.5), 14.0);
    var col = palette(z * 0.06 + a + t * 0.02) * (0.04 + 1.5 * stripes * s + ring * (0.5 + u.lv.w));
    col *= smoothstep(0.0, 0.3, r);
    col += palette(t * 0.05 + 0.5) * u.lv.w * 0.8 * exp(-r * 7.0);
    return col;
}

fn halo(q: vec2f, t: f32) -> vec3f {
    let r = length(q);
    let a = atan2(q.y, q.x) / TAU + 0.5;
    let x = abs(fract(a + t * 0.01) * 2.0 - 1.0);
    let seg = (floor(x * 48.0) + 0.5) / 48.0;
    let gap = smoothstep(0.08, 0.2, fract(x * 48.0)) * smoothstep(0.92, 0.8, fract(x * 48.0));
    let base = 0.2 + 0.035 * u.lv.x + 0.04 * u.lv.w;
    let h = bin(seg) * 0.3;
    let inside = step(base, r) * step(r, base + h) * gap;
    let tip = exp(-abs(r - (base + h)) * 60.0) * gap;
    var col = palette(seg * 0.8 + t * 0.03) * (inside * (0.4 + 0.9 * (r - base) / max(h, 0.01)) + tip * 1.2);
    // The rim and a pulse inside it.
    col += palette(t * 0.04) * exp(-abs(r - base + 0.012) * 140.0) * (0.6 + u.lv.w);
    col += palette(t * 0.04 + 0.33) * exp(-r * 9.0) * (0.15 + 0.6 * u.lv.w);
    // Faint stars drifting outwards.
    let g = q * 40.0 / (1.0 + 0.15 * fract(t * 0.05));
    let star = step(0.985, hash(floor(g))) * smoothstep(0.14, 0.0, length(fract(g) - 0.5)) * (0.3 + 0.7 * u.lv.z);
    col += vec3f(star) * 0.5 * smoothstep(base + 0.1, 0.9, r);
    return col;
}

fn ribbon(uv: vec2f, q: vec2f, t: f32) -> vec3f {
    // A dim grid behind the trace.
    let g = abs(fract(uv * vec2f(16.0, 9.0)) - 0.5);
    var col = palette(0.5) * 0.05 * (smoothstep(0.47, 0.5, g.x) + smoothstep(0.47, 0.5, g.y));
    let amp = 0.38 + 0.25 * u.lv.x;
    for (var k = 0; k < 3; k = k + 1) {
        let kf = f32(k);
        let y = wave(uv.x) * amp * (1.0 - kf * 0.22) + sin(uv.x * 9.0 + t * (1.0 + kf)) * 0.02 * kf;
        let d = abs((uv.y - 0.5) * 2.0 - y);
        let w = 1.0 + 3.0 * u.lv.w;
        let line = exp(-d * 260.0 / w) + 0.35 * exp(-d * 24.0);
        col += palette(uv.x * 0.6 + t * 0.06 + kf * 0.25) * line * (1.0 - kf * 0.3);
    }
    // Spectrum glow along the floor.
    col += palette(uv.x + t * 0.02) * bin(uv.x) * 0.5 * smoothstep(0.35, 0.0, 1.0 - uv.y);
    return col;
}

fn plasma(q: vec2f, t: f32) -> vec3f {
    let k = 5.0 + u.lv.y * 6.0;
    var v = sin(q.x * k + t);
    v += sin(q.y * k * 1.3 + t * 1.7);
    v += sin((q.x + q.y) * k * 0.7 + t * 0.5);
    let c = q * k + vec2f(sin(t * 0.33), cos(t * 0.41)) * 3.0;
    v += sin(length(c) + t);
    var col = palette(v * 0.18 + t * 0.04 + u.lv.z * 0.2) * (0.35 + 0.9 * u.lv.x);
    col += vec3f(1.0) * u.lv.w * 0.25 * smoothstep(0.6, 1.0, sin(v * 3.0));
    return col;
}

fn sd_rounded(p: vec2f, half: vec2f, r: f32) -> f32 {
    let d = abs(p) - half + vec2f(r);
    return length(max(d, vec2f(0.0))) + min(max(d.x, d.y), 0.0) - r;
}

fn encode(c: vec3f) -> vec3f {
    let lo = c * 12.92;
    let hi = 1.055 * pow(c, vec3f(1.0 / 2.4)) - 0.055;
    return select(hi, lo, c <= vec3f(0.0031308));
}

@fragment
fn fs(in: V) -> @location(0) vec4f {
    let local = in.pos.xy - u.rect.xy;
    let uv = local / u.rect.zw;
    let aspect = u.rect.z / u.rect.w;
    let q = (uv - 0.5) * vec2f(aspect, 1.0);
    let t = u.view.z;
    let mode = i32(u.view.w + 0.5);
    var col: vec3f;
    switch mode {
        case 1: { col = halo(q, t); }
        case 2: { col = ribbon(uv, q, t); }
        case 3: { col = plasma(q, t); }
        default: { col = aurora(q, t); }
    }
    // Vignette, then a soft shoulder instead of clipping.
    col *= 1.0 - 0.45 * smoothstep(0.35, 1.0, length(q));
    col = 1.0 - exp(-col * 1.4);
    let edge = sd_rounded(local - u.rect.zw * 0.5, u.rect.zw * 0.5, u.misc.z);
    let a = clamp(0.5 - edge, 0.0, 1.0);
    var out = col;
    if u.misc.x > 0.5 { out = encode(col); }
    return vec4f(out * a, a);
}
"#;

#[repr(C)]
#[derive(Clone, Copy)]
struct Uniforms {
    rect: [f32; 4],
    view: [f32; 4],
    lv: [f32; 4],
    misc: [f32; 4],
    c: [[f32; 4]; 3],
    bins: [f32; BINS],
    wave: [f32; WAVE],
}

struct Gpu {
    key: DeviceKey,
    module: wgpu::ShaderModule,
    layout: wgpu::PipelineLayout,
    buffer: wgpu::Buffer,
    bind: wgpu::BindGroup,
    pipes: HashMap<wgpu::TextureFormat, wgpu::RenderPipeline>,
}

impl Gpu {
    fn new(device: &wgpu::Device, key: DeviceKey) -> Gpu {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("nightjar-viz"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("nightjar-viz"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("nightjar-viz"),
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("nightjar-viz"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("nightjar-viz"),
            layout: &bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        });
        Gpu {
            key,
            module,
            layout,
            buffer,
            bind,
            pipes: HashMap::new(),
        }
    }

    fn pipeline(
        &mut self,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
    ) -> &wgpu::RenderPipeline {
        let (module, layout) = (&self.module, &self.layout);
        self.pipes.entry(format).or_insert_with(|| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("nightjar-viz"),
                layout: Some(layout),
                vertex: wgpu::VertexState {
                    module,
                    entry_point: Some("vs"),
                    buffers: &[],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module,
                    entry_point: Some("fs"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            })
        })
    }
}

#[derive(Default)]
pub struct Viz {
    gpu: Option<Gpu>,
    /// Visual time: runs faster with the music's energy, stops when playback stops.
    time: f32,
    playing: bool,
}

fn bytes(u: &Uniforms) -> &[u8] {
    // Plain f32 arrays with no padding: safe to view as bytes.
    unsafe {
        std::slice::from_raw_parts(
            (u as *const Uniforms).cast::<u8>(),
            std::mem::size_of::<Uniforms>(),
        )
    }
}

impl Compositor<State> for Viz {
    /// Never: frames come from `poll` (about 30 a second while playing, via `State::tick`),
    /// not from the display link, which would draw at the display's 60 or 120 Hz.
    fn animating(&self) -> bool {
        false
    }

    fn render(&mut self, f: &mut Frame<'_, State>) {
        let s = &f.rt.state;
        let l = s.levels().clone();
        let k = skin(s.skin);
        self.playing = s.now_play() == Play::Playing || s.demo;
        let energy = (l.bass * 0.6 + l.mid * 0.3 + l.treble * 0.1).clamp(0.0, 1.0);
        if self.playing {
            self.time += f.dt.min(0.1) as f32 * (0.35 + 1.4 * energy);
        }
        if s.demo && self.time == 0.0 {
            self.time = 7.3;
        }
        let mut u = Uniforms {
            rect: [0.0; 4],
            view: [0.0, 0.0, self.time, s.viz as f32],
            lv: [l.bass, l.mid, l.treble, l.beat],
            misc: [0.0, self.playing as u8 as f32, 0.0, energy],
            c: [rgb(k.viz[0]), rgb(k.viz[1]), rgb(k.viz[2])],
            bins: l.bins,
            wave: l.wave,
        };

        let (device, queue) = (f.device, f.queue);
        let key = f.device_key();
        if self.gpu.as_ref().is_none_or(|g| g.key != key) {
            self.gpu = Some(Gpu::new(device, key));
        }
        let gpu = self.gpu.as_mut().unwrap();
        let clear = f.rt.clear_color();
        f.ui.prepare(device, queue, &mut f.rt.text.atlas, &f.rt.instances, f.size);
        f.ui.draw_with(f.encoder, f.target, clear, &mut |pass, site| {
            if site.mark != VIZ_MARK {
                return;
            }
            let r = site.instance.rect;
            u.rect = [r[0] - site.origin[0], r[1] - site.origin[1], r[2], r[3]];
            u.view[0] = site.size[0] as f32;
            u.view[1] = site.size[1] as f32;
            u.view[3] = site.data[0];
            u.misc[0] = site.srgb_blend as u8 as f32;
            u.misc[2] = 15.0 * site.scale;
            queue.write_buffer(&gpu.buffer, 0, bytes(&u));
            let pipe = gpu.pipeline(device, site.format).clone();
            pass.set_pipeline(&pipe);
            pass.set_bind_group(0, &gpu.bind, &[]);
            pass.draw(0..6, 0..1);
        });
    }
}
