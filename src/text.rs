//! Text rendering pipeline (README M2): fontconfig discovery, FreeType
//! rasterization, glyph atlas, and the three-zoom crispness strategy.

use std::collections::HashMap;

use freetype::Library;
use glow::HasContext;

/// Font render style flags, part of the atlas key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct StyleFlags {
    pub bold: bool,
    pub italic: bool,
}

/// Color mode, part of the atlas key (mono now; emoji/COLR later).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ColorMode {
    /// Single-channel alpha, tinted by the foreground color at draw time.
    Mono,
}

/// Subpixel horizontal position bucket (0..=4, 1/5 em steps) — part of the
/// atlas key for crisp text at fractional positions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SubpixelBucket(pub u8);

impl SubpixelBucket {
    pub fn from_fraction(frac: f64) -> Self {
        Self((frac.clamp(0.0, 0.999) * 5.0).floor() as u8)
    }
    pub fn offset(&self) -> f64 {
        self.0 as f64 / 5.0
    }
}

/// Complete glyph atlas cache key per README §4.4.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GlyphKey {
    pub font_id: u32,
    pub glyph_id: u32,
    pub pixel_size: u32,
    pub subpixel: SubpixelBucket,
    pub style: StyleFlags,
    pub color_mode: ColorMode,
}

/// Rasterized glyph bitmap (single-channel coverage).
#[derive(Debug, Clone)]
pub struct GlyphBitmap {
    pub width: u32,
    pub height: u32,
    /// Bitmap origin offset from the pen position.
    pub left: i32,
    pub top: i32,
    /// Pen advance in 26.6 fixed point.
    pub advance: i64,
    pub data: Vec<u8>,
}

/// A font face registered with the system.
pub struct Face {
    pub font_id: u32,
    pub family: String,
    face: freetype::Face,
}

impl Face {
    /// Rasterize a character at the requested pixel size and subpixel offset.
    pub fn rasterize(
        &mut self,
        ch: char,
        pixel_size: u32,
        subpixel: SubpixelBucket,
    ) -> Option<(u32, GlyphBitmap)> {
        self.face.set_pixel_sizes(0, pixel_size).ok()?;
        // Subpixel offset: shift the pen position by the bucket fraction.
        let mut matrix = freetype::Matrix {
            xx: 0x10000,
            xy: 0,
            yx: 0,
            yy: 0x10000,
        };
        let mut delta = freetype::Vector {
            x: (subpixel.offset() * 64.0) as freetype::ffi::FT_Pos,
            y: 0,
        };
        self.face.set_transform(&mut matrix, &mut delta);
        self.face
            .load_char(
                ch as usize,
                freetype::face::LoadFlag::RENDER | freetype::face::LoadFlag::TARGET_NORMAL,
            )
            .ok()?;
        let slot = self.face.glyph();
        let bitmap = slot.bitmap();
        let (w, h) = (bitmap.width() as u32, bitmap.rows() as u32);
        let src = bitmap.buffer();
        // LCD target yields 3x coverage; normalize to mono coverage by
        // averaging channels if wide, else copy directly.
        let lcd = bitmap.pixel_mode() == Ok(freetype::bitmap::PixelMode::Lcd);
        let bpp: usize = if lcd { 3 } else { 1 };
        let mut data = Vec::with_capacity((w * h) as usize);
        for y in 0..h as usize {
            for x in 0..w as usize {
                let base = y * w as usize * bpp + x * bpp;
                let cov = match bpp {
                    3 => {
                        let r = src.get(base).copied().unwrap_or(0) as u32;
                        let g = src.get(base + 1).copied().unwrap_or(0) as u32;
                        let b = src.get(base + 2).copied().unwrap_or(0) as u32;
                        ((r + g + b) / 3) as u8
                    }
                    _ => src.get(base).copied().unwrap_or(0),
                };
                data.push(cov);
            }
        }
        let glyph_id = self.face.get_char_index(ch as usize).unwrap_or(0);
        Some((
            glyph_id,
            GlyphBitmap {
                width: w,
                height: h,
                left: slot.bitmap_left(),
                top: slot.bitmap_top(),
                advance: slot.advance().x,
                data,
            },
        ))
    }
}

/// Font discovery + face management. Uses fontconfig for family lookup.
pub struct FontSystem {
    ft: Library,
    faces: Vec<Face>,
    /// fontconfig lookup cache: family -> font file path
    paths: HashMap<String, String>,
    next_font_id: u32,
}

impl FontSystem {
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            ft: Library::init().map_err(|e| e.to_string())?,
            faces: Vec::new(),
            paths: HashMap::new(),
            next_font_id: 1,
        })
    }

    /// Resolve a font family to a file path via fontconfig.
    pub fn font_path(&mut self, family: &str) -> Option<String> {
        if let Some(p) = self.paths.get(family) {
            return Some(p.clone());
        }
        let fc = fontconfig::Fontconfig::new()?;
        let matched = fc.find(family, None)?;
        let path = matched.path.to_string_lossy().to_string();
        self.paths.insert(family.to_string(), path.clone());
        Some(path)
    }

    /// Load a family as a face. Falls back to fontconfig's default match.
    pub fn load_family(&mut self, family: &str) -> Result<u32, String> {
        let path = self
            .font_path(family)
            .or_else(|| self.font_path("monospace"))
            .ok_or_else(|| format!("no font found for family '{family}'"))?;
        let face = self.ft.new_face(&path, 0).map_err(|e| e.to_string())?;
        let font_id = self.next_font_id;
        self.next_font_id += 1;
        self.faces.push(Face {
            font_id,
            family: family.to_string(),
            face,
        });
        Ok(font_id)
    }

    pub fn face_mut(&mut self, font_id: u32) -> Option<&mut Face> {
        self.faces.iter_mut().find(|f| f.font_id == font_id)
    }
}

/// GL-side glyph atlas: R8 textures with row-based shelf packing, keyed by
/// `GlyphKey`.
pub struct Atlas {
    gl_texture: glow::Texture,
    size: u32,
    /// Packing cursor (y offset of current shelf, x offset within shelf,
    /// height of tallest glyph on current shelf).
    cursor: (u32, u32, u32),
    /// Atlas-space rect per key: (x, y, w, h) in texels.
    placements: HashMap<GlyphKey, (u32, u32, u32, u32)>,
}

impl Atlas {
    /// Create an empty atlas backed by an R8 texture.
    ///
    /// # Safety
    /// `gl` must be a valid, current OpenGL context.
    pub unsafe fn new(gl: &glow::Context, size: u32) -> Self {
        let texture = gl.create_texture().expect("atlas texture");
        gl.bind_texture(glow::TEXTURE_2D, Some(texture));
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::R8 as i32,
            size as i32,
            size as i32,
            0,
            glow::RED,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(None),
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MIN_FILTER,
            glow::LINEAR as i32,
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MAG_FILTER,
            glow::LINEAR as i32,
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_WRAP_S,
            glow::CLAMP_TO_EDGE as i32,
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_WRAP_T,
            glow::CLAMP_TO_EDGE as i32,
        );
        gl.bind_texture(glow::TEXTURE_2D, None);
        Self {
            gl_texture: texture,
            size,
            cursor: (0, 0, 0),
            placements: HashMap::new(),
        }
    }

    pub fn get(&self, key: &GlyphKey) -> Option<(u32, u32, u32, u32)> {
        self.placements.get(key).copied()
    }

    /// Insert a glyph bitmap; returns its atlas placement.
    ///
    /// # Safety
    /// `gl` must be a valid, current OpenGL context.
    pub unsafe fn insert(
        &mut self,
        gl: &glow::Context,
        key: GlyphKey,
        bitmap: &GlyphBitmap,
    ) -> (u32, u32, u32, u32) {
        let (mut px, mut py, mut shelf_h) = self.cursor;
        let (w, h) = (bitmap.width.max(1), bitmap.height.max(1));
        // Row-advance when the current shelf is full.
        if px + w + 1 > self.size {
            px = 0;
            py += shelf_h + 1;
            shelf_h = 0;
        }
        // Out of vertical space: in production this grows to a second page;
        // for now evict and restart (damage tracking rebuilds lazily).
        if py + h + 1 > self.size {
            self.placements.clear();
            px = 0;
            py = 0;
            shelf_h = 0;
        }

        gl.bind_texture(glow::TEXTURE_2D, Some(self.gl_texture));
        gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
        gl.tex_sub_image_2d(
            glow::TEXTURE_2D,
            0,
            px as i32,
            py as i32,
            w as i32,
            h as i32,
            glow::RED,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(Some(&bitmap.data)),
        );
        gl.bind_texture(glow::TEXTURE_2D, None);

        let placement = (px, py, w, h);
        self.placements.insert(key, placement);
        px += w + 1;
        shelf_h = shelf_h.max(h);
        self.cursor = (px, py, shelf_h);
        placement
    }

    pub fn texture(&self) -> glow::Texture {
        self.gl_texture
    }
}

/// Pixel-size selection for the three-zoom strategy (README §4.3):
/// - far (zoom < 1): rasterize at the cached layer size (1x base) and scale
///   down via the layer texture;
/// - near (1 <= zoom <= 8): rasterize glyphs directly at screen size;
/// - very large (zoom > 8): rasterize at high resolution for sharpness.
pub fn choose_pixel_size(base_px: u32, zoom: f64) -> u32 {
    if zoom < 1.0 {
        base_px
    } else if zoom <= 8.0 {
        ((base_px as f64 * zoom).round() as u32).clamp(1, 256)
    } else {
        ((base_px as f64 * zoom.min(32.0)).round() as u32).clamp(1, 512)
    }
}

const GLYPH_VERTEX_SRC: &str = r#"
#version 330 core
layout (location = 0) in vec2 a_pos;   // screen pixels
layout (location = 1) in vec2 a_uv;    // atlas texels
layout (location = 2) in vec4 a_color; // tint (rgb) + opacity (a)
uniform vec2 u_viewport;
out vec2 v_uv;
out vec4 v_color;
void main() {
    vec2 ndc = vec2(2.0 * a_pos.x / u_viewport.x, -2.0 * a_pos.y / u_viewport.y);
    gl_Position = vec4(ndc, 0.0, 1.0);
    v_uv = a_uv / vec2(textureSize(u_atlas, 0));
    v_color = a_color;
}
"#;

const GLYPH_FRAGMENT_SRC: &str = r#"
#version 330 core
in vec2 v_uv;
in vec4 v_color;
uniform sampler2D u_atlas;
out vec4 frag_color;
void main() {
    float alpha = texture(u_atlas, v_uv).r;
    frag_color = vec4(v_color.rgb, v_color.a * alpha);
}
"#;

/// Screen-space glyph renderer sampling the shared glyph atlas.
/// Glyphs are rasterized at their on-screen pixel size, so text stays crisp
/// through the whole zoom range (three-zoom strategy, README §4.3).
pub struct TextRenderer {
    program: glow::Program,
    vao: glow::VertexArray,
    vbo: glow::Buffer,
    verts: Vec<f32>,
}

impl TextRenderer {
    /// # Safety
    /// `gl` must be a valid, current OpenGL context.
    pub unsafe fn new(gl: &glow::Context) -> Result<Self, String> {
        let program = gl.create_program()?;
        let shader = |kind: u32, src: &str| -> Result<glow::Shader, String> {
            let s = gl.create_shader(kind)?;
            gl.shader_source(s, src);
            gl.compile_shader(s);
            if !gl.get_shader_compile_status(s) {
                return Err(gl.get_shader_info_log(s));
            }
            Ok(s)
        };
        gl.attach_shader(program, shader(glow::VERTEX_SHADER, GLYPH_VERTEX_SRC)?);
        gl.attach_shader(program, shader(glow::FRAGMENT_SHADER, GLYPH_FRAGMENT_SRC)?);
        gl.link_program(program);
        if !gl.get_program_link_status(program) {
            return Err(gl.get_program_info_log(program));
        }

        let vao = gl.create_vertex_array()?;
        let vbo = gl.create_buffer()?;
        gl.bind_vertex_array(Some(vao));
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
        let stride = 8 * std::mem::size_of::<f32>() as i32;
        gl.enable_vertex_attrib_array(0);
        gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, stride, 0);
        gl.enable_vertex_attrib_array(1);
        gl.vertex_attrib_pointer_f32(1, 2, glow::FLOAT, false, stride, 16);
        gl.enable_vertex_attrib_array(2);
        gl.vertex_attrib_pointer_f32(2, 4, glow::FLOAT, false, stride, 32);
        gl.bind_vertex_array(None);
        gl.bind_buffer(glow::ARRAY_BUFFER, None);

        Ok(Self {
            program,
            vao,
            vbo,
            verts: Vec::new(),
        })
    }

    /// Rasterize, atlas-cache, and queue a string for drawing. `world` is
    /// the world-space pen origin; glyphs are rasterized at their on-screen
    /// pixel size (`choose_pixel_size`), so they stay crisp while zooming.
    ///
    /// # Safety
    /// `gl` must be a valid, current OpenGL context.
    #[allow(clippy::too_many_arguments)]
    pub unsafe fn queue_string(
        &mut self,
        gl: &glow::Context,
        atlas: &mut Atlas,
        face: &mut Face,
        world: (f64, f64),
        cam: (f64, f64, f64),
        base_px: u32,
        text: &str,
        color: (f32, f32, f32, f32),
    ) {
        let (cam_x, cam_y, zoom) = cam;
        let screen_px = choose_pixel_size(base_px, zoom);
        let mut pen_x = (world.0 - cam_x) * zoom;
        let pen_y = (world.1 - cam_y) * zoom;
        for ch in text.chars() {
            if ch == ' ' {
                pen_x += screen_px as f64 * 0.55;
                continue;
            }
            let subpixel = SubpixelBucket::from_fraction(pen_x.fract());
            let Some((glyph_id, bm)) = face.rasterize(ch, screen_px, subpixel) else {
                continue;
            };
            let key = GlyphKey {
                font_id: face.font_id,
                glyph_id,
                pixel_size: screen_px,
                subpixel,
                style: StyleFlags::default(),
                color_mode: ColorMode::Mono,
            };
            let (ax, ay, aw, ah) = match atlas.get(&key) {
                Some(p) => p,
                None => atlas.insert(gl, key, &bm),
            };
            let x0 = pen_x + bm.left as f64;
            let y0 = pen_y - bm.top as f64;
            let (w, h) = (bm.width as f64, bm.height as f64);
            self.verts.extend_from_slice(&glyph_quad(
                x0 as f32,
                y0 as f32,
                w as f32,
                h as f32,
                ax as f32,
                ay as f32,
                (ax + aw) as f32,
                (ay + ah) as f32,
                color,
            ));
            pen_x += bm.advance as f64 / 64.0;
        }
    }

    /// Upload and draw all queued glyph quads in one draw call.
    ///
    /// # Safety
    /// `gl` must be a valid, current OpenGL context.
    pub unsafe fn flush(&mut self, gl: &glow::Context, atlas: &Atlas, viewport: (f32, f32)) {
        if self.verts.is_empty() {
            return;
        }
        gl.viewport(0, 0, viewport.0 as i32, viewport.1 as i32);
        gl.enable(glow::BLEND);
        gl.blend_func_separate(
            glow::SRC_ALPHA,
            glow::ONE_MINUS_SRC_ALPHA,
            glow::ONE,
            glow::ONE_MINUS_SRC_ALPHA,
        );
        gl.use_program(Some(self.program));
        gl.uniform_2_f32(
            gl.get_uniform_location(self.program, "u_viewport").as_ref(),
            viewport.0,
            viewport.1,
        );
        gl.active_texture(glow::TEXTURE0);
        gl.bind_texture(glow::TEXTURE_2D, Some(atlas.texture()));
        gl.uniform_1_i32(gl.get_uniform_location(self.program, "u_atlas").as_ref(), 0);
        gl.bind_vertex_array(Some(self.vao));
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vbo));
        gl.buffer_data_u8_slice(
            glow::ARRAY_BUFFER,
            std::slice::from_raw_parts(
                self.verts.as_ptr().cast(),
                self.verts.len() * std::mem::size_of::<f32>(),
            ),
            glow::DYNAMIC_DRAW,
        );
        gl.draw_arrays(glow::TRIANGLES, 0, (self.verts.len() / 8) as i32);
        gl.bind_vertex_array(None);
        gl.bind_buffer(glow::ARRAY_BUFFER, None);
        gl.bind_texture(glow::TEXTURE_2D, None);
        self.verts.clear();
    }
}

/// Build the 6 vertices (pos2, uv2, color4) for one glyph quad.
#[allow(clippy::too_many_arguments)]
fn glyph_quad(
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    u0: f32,
    v0: f32,
    u1: f32,
    v1: f32,
    color: (f32, f32, f32, f32),
) -> [f32; 48] {
    let (r, g, b, a) = color;
    let (x1, y1, u1v, v1v) = (x + w, y + h, u1, v1);
    [
        x, y, u0, v0, r, g, b, a, x1, y, u1v, v0, r, g, b, a, x, y1, u0, v1v, r, g, b, a, x1, y,
        u1v, v0, r, g, b, a, x1, y1, u1v, v1v, r, g, b, a, x, y1, u0, v1v, r, g, b, a,
    ]
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_choose_pixel_size() {
        assert_eq!(choose_pixel_size(14, 0.5), 14);
        assert_eq!(choose_pixel_size(14, 1.0), 14);
        assert_eq!(choose_pixel_size(14, 4.0), 56);
        assert_eq!(choose_pixel_size(14, 16.0), 224);
    }

    #[test]
    fn test_subpixel_bucket() {
        assert_eq!(SubpixelBucket::from_fraction(0.0).0, 0);
        assert_eq!(SubpixelBucket::from_fraction(0.5).0, 2);
        assert_eq!(SubpixelBucket::from_fraction(0.99).0, 4);
    }

    #[test]
    fn test_atlas_key_roundtrip() {
        use std::collections::HashMap;
        let key = GlyphKey {
            font_id: 1,
            glyph_id: 65,
            pixel_size: 24,
            subpixel: SubpixelBucket(2),
            style: StyleFlags {
                bold: true,
                italic: false,
            },
            color_mode: ColorMode::Mono,
        };
        let mut map = HashMap::new();
        map.insert(key.clone(), (0u32, 0, 8, 8));
        assert!(map.contains_key(&key));
    }
}
