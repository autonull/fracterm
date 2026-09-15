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
        // Normalize every FreeType pixel mode to tight grayscale coverage.
        // `buffer()` spans `|pitch| * rows` (rows may be padded and pitch
        // may be negative for up-flow bitmaps), so a pitch-aware copy is
        // required for legible glyphs.
        let mode = bitmap
            .pixel_mode()
            .unwrap_or(freetype::bitmap::PixelMode::None);
        let raw_w = bitmap.width().max(0) as u32;
        let raw_h = bitmap.rows().max(0) as u32;
        let pitch = bitmap.pitch();
        let src = bitmap.buffer();
        let (w, h, data) = normalize_bitmap(raw_w, raw_h, pitch, mode, src);
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
        eprintln!("[fracterm] font '{family}' -> {path}");
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

    /// Load a primary family plus best-effort fallbacks for glyphs the
    /// primary lacks (symbols, prompt glyphs like U+276F). Returns all
    /// loaded ids, primary first; requires at least one success.
    pub fn load_family_stack(&mut self, families: &[&str]) -> Result<Vec<u32>, String> {
        let mut ids = Vec::new();
        for family in families {
            match self.load_family(family) {
                Ok(id) => ids.push(id),
                Err(e) => eprintln!("[fracterm] fallback font '{family}' unavailable: {e}"),
            }
        }
        if ids.is_empty() {
            return Err("no fonts loaded".to_string());
        }
        Ok(ids)
    }

    /// Rasterize through the fallback stack: the primary face first, then
    /// fallbacks. The first face whose glyph id is non-zero (really present,
    /// not .notdef) wins; otherwise the primary's .notdef box is returned
    /// so the pen still advances with a visible placeholder. Returns the
    /// supplying face's id and glyph id for atlas keying.
    pub fn rasterize(
        &mut self,
        font_id: u32,
        ch: char,
        pixel_size: u32,
        subpixel: SubpixelBucket,
    ) -> Option<(u32, u32, GlyphBitmap)> {
        let primary_idx = self.faces.iter().position(|f| f.font_id == font_id);
        let mut order: Vec<usize> = (0..self.faces.len()).collect();
        if let Some(p) = primary_idx {
            order.retain(|&i| i != p);
            order.insert(0, p);
        }
        let mut notdef: Option<(u32, u32, GlyphBitmap)> = None;
        for idx in order {
            let face = &mut self.faces[idx];
            let Some((gid, bm)) = face.rasterize(ch, pixel_size, subpixel) else {
                continue;
            };
            if gid != 0 {
                return Some((face.font_id, gid, bm));
            }
            if notdef.is_none() {
                notdef = Some((face.font_id, gid, bm));
            }
        }
        notdef
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
        // Zero the atlas: gaps between packed glyphs must sample as
        // transparent under LINEAR filtering, never uninitialized memory.
        gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
        let blank = vec![0u8; (size * size) as usize];
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::R8 as i32,
            size as i32,
            size as i32,
            0,
            glow::RED,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(Some(&blank)),
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
        // Empty glyphs carry no coverage: degenerate placement, no upload.
        if bitmap.width == 0 || bitmap.height == 0 || bitmap.data.is_empty() {
            return (0, 0, 0, 0);
        }
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

    pub fn size(&self) -> u32 {
        self.size
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

/// Measured font metrics at a pixel size (all values in pixels).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FontMetrics {
    pub ascender: f64,
    pub descender: f64,
    pub line_height: f64,
    pub max_advance: f64,
}

impl FontMetrics {
    /// Grid cell size for terminal dashboards: full advance by full height.
    pub fn cell_size(&self) -> (f64, f64) {
        (
            self.max_advance.ceil().max(2.0),
            self.line_height.ceil().max(4.0),
        )
    }
}

impl Face {
    /// Measure the face at a pixel size for grid layout.
    ///
    /// The advance is measured from a real glyph ('M'), not
    /// `FT_Size_Metrics::max_advance`, which some faces report at a
    /// multiple of the true advance (observed 3x on Noto Sans Mono:
    /// 27px vs the real 9px). A wrong cell width sizes the terminal
    /// node wrong, forces a shrunken camera fit, and turns text into
    /// an illegible smear.
    pub fn font_metrics(&mut self, pixel_size: u32) -> Option<FontMetrics> {
        self.face.set_pixel_sizes(0, pixel_size).ok()?;
        let m = self.face.size_metrics()?;
        self.face
            .load_char('M' as usize, freetype::face::LoadFlag::empty())
            .ok()?;
        let advance = self.face.glyph().advance().x as f64 / 64.0;
        if !advance.is_finite() || advance <= 0.0 {
            return None;
        }
        Some(FontMetrics {
            ascender: m.ascender as f64 / 64.0,
            descender: m.descender as f64 / 64.0,
            line_height: m.height as f64 / 64.0,
            max_advance: advance,
        })
    }
}

/// Normalize any FreeType bitmap to tight grayscale coverage.
///
/// Handles row pitch (including negative/up-flow pitch), Mono bit packing,
/// Gray2/Gray4 scaling, LCD/LCDV decimation and BGRA alpha. LCD bitmaps are
/// three times wider (LCDV: taller) than the glyph; the returned size is the
/// true glyph size.
pub fn normalize_bitmap(
    width: u32,
    rows: u32,
    pitch: i32,
    mode: freetype::bitmap::PixelMode,
    src: &[u8],
) -> (u32, u32, Vec<u8>) {
    use freetype::bitmap::PixelMode as PM;
    if width == 0 || rows == 0 || src.is_empty() || mode == PM::None {
        return (0, 0, Vec::new());
    }
    let stride = pitch.unsigned_abs() as usize;
    if stride == 0 {
        return (0, 0, Vec::new());
    }
    let flip = pitch < 0;
    let h = rows as usize;
    let row_off = |logical: usize| -> usize {
        let phys = if flip { h - 1 - logical } else { logical };
        phys * stride
    };
    match mode {
        PM::None => (0, 0, Vec::new()),
        PM::Gray => {
            let mut out = Vec::with_capacity((width * rows) as usize);
            for y in 0..h {
                let base = row_off(y);
                for x in 0..width as usize {
                    out.push(src.get(base + x).copied().unwrap_or(0));
                }
            }
            (width, rows, out)
        }
        PM::Mono => {
            let mut out = Vec::with_capacity((width * rows) as usize);
            for y in 0..h {
                let base = row_off(y);
                for x in 0..width as usize {
                    let b = src.get(base + x / 8).copied().unwrap_or(0);
                    let bit = 0x80u8 >> (x % 8);
                    out.push(if b & bit != 0 { 255 } else { 0 });
                }
            }
            (width, rows, out)
        }
        PM::Gray2 => {
            let mut out = Vec::with_capacity((width * rows) as usize);
            for y in 0..h {
                let base = row_off(y);
                for x in 0..width as usize {
                    let b = src.get(base + x / 4).copied().unwrap_or(0);
                    let shift = 6 - 2 * (x % 4);
                    out.push((((b >> shift) & 3) as u16 * 85) as u8);
                }
            }
            (width, rows, out)
        }
        PM::Gray4 => {
            let mut out = Vec::with_capacity((width * rows) as usize);
            for y in 0..h {
                let base = row_off(y);
                for x in 0..width as usize {
                    let b = src.get(base + x / 2).copied().unwrap_or(0);
                    let shift = 4 - 4 * (x % 2);
                    out.push((((b >> shift) & 15) as u16 * 17) as u8);
                }
            }
            (width, rows, out)
        }
        PM::Lcd => {
            let gw = width / 3;
            if gw == 0 {
                return (0, 0, Vec::new());
            }
            let mut out = Vec::with_capacity((gw * rows) as usize);
            for y in 0..h {
                let base = row_off(y);
                for x in 0..gw as usize {
                    let r = src.get(base + x * 3).copied().unwrap_or(0) as u32;
                    let g = src.get(base + x * 3 + 1).copied().unwrap_or(0) as u32;
                    let b = src.get(base + x * 3 + 2).copied().unwrap_or(0) as u32;
                    out.push(((r + g + b) / 3) as u8);
                }
            }
            (gw, rows, out)
        }
        PM::LcdV => {
            let gh = rows / 3;
            if gh == 0 {
                return (0, 0, Vec::new());
            }
            let w = width as usize;
            let mut out = Vec::with_capacity((width * gh) as usize);
            for y in 0..gh as usize {
                for x in 0..w {
                    let mut acc = 0u32;
                    for k in 0..3 {
                        let phys = if flip { h - 1 - (y * 3 + k) } else { y * 3 + k };
                        acc += src.get(phys * stride + x).copied().unwrap_or(0) as u32;
                    }
                    out.push((acc / 3) as u8);
                }
            }
            (width, gh, out)
        }
        PM::Bgra => {
            let mut out = Vec::with_capacity((width * rows) as usize);
            for y in 0..h {
                let base = row_off(y);
                for x in 0..width as usize {
                    out.push(src.get(base + x * 4 + 3).copied().unwrap_or(0));
                }
            }
            (width, rows, out)
        }
    }
}

const GLYPH_VERTEX_SRC: &str = r#"
#version 330 core
layout (location = 0) in vec2 a_pos;   // screen pixels
layout (location = 1) in vec2 a_uv;    // atlas texels
layout (location = 2) in vec4 a_color; // tint (rgb) + opacity (a)
uniform vec2 u_viewport;
uniform vec2 u_atlas_size;
out vec2 v_uv;
out vec4 v_color;
void main() {
    vec2 ndc = vec2(2.0 * a_pos.x / u_viewport.x - 1.0, 1.0 - 2.0 * a_pos.y / u_viewport.y);
    gl_Position = vec4(ndc, 0.0, 1.0);
    // Atlas size arrives as a plain uniform: sampler queries in the vertex
    // stage are fragile across drivers (observed: NaN UVs -> every glyph
    // quad sampling one bright texel -> solid illegible blocks).
    // Half-texel offset anchors LINEAR filtering on texel centers so glyph
    // edges never bleed from neighboring atlas cells.
    v_uv = (a_uv + vec2(0.5)) / u_atlas_size;
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

/// Text vertex layout: pos2 + uv2 + color4, packed as consecutive f32s.
/// Attribute offsets are in BYTES: uv starts after pos (2 floats = 8B),
/// color after pos+uv (4 floats = 16B). Getting these wrong shifts every
/// attribute (observed: uv reading color → solid white glyph blocks).
const TEXT_VERT_FLOATS: usize = 8;
const TEXT_UV_OFFSET_BYTES: i32 = 2 * std::mem::size_of::<f32>() as i32;
const TEXT_COLOR_OFFSET_BYTES: i32 = 4 * std::mem::size_of::<f32>() as i32;

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
        // Vertex layout is pos2 + uv2 + color4; offsets are in BYTES.
        let stride = TEXT_VERT_FLOATS as i32 * std::mem::size_of::<f32>() as i32;
        gl.enable_vertex_attrib_array(0);
        gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, stride, 0);
        gl.enable_vertex_attrib_array(1);
        gl.vertex_attrib_pointer_f32(1, 2, glow::FLOAT, false, stride, TEXT_UV_OFFSET_BYTES);
        gl.enable_vertex_attrib_array(2);
        gl.vertex_attrib_pointer_f32(2, 4, glow::FLOAT, false, stride, TEXT_COLOR_OFFSET_BYTES);
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
        fonts: &mut FontSystem,
        font_id: u32,
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
            let Some((supply_id, glyph_id, bm)) = fonts.rasterize(font_id, ch, screen_px, subpixel)
            else {
                continue;
            };
            // Empty glyphs (no coverage) still advance the pen.
            if bm.width == 0 || bm.height == 0 || bm.data.is_empty() {
                pen_x += bm.advance as f64 / 64.0;
                continue;
            }
            let key = GlyphKey {
                font_id: supply_id,
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
        let atlas_px = atlas.size() as f32;
        gl.uniform_2_f32(
            gl.get_uniform_location(self.program, "u_atlas_size")
                .as_ref(),
            atlas_px,
            atlas_px,
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

    #[test]
    fn test_vertex_layout_offsets() {
        // Regression test: attribute offsets are BYTES into pos2+uv2+color4.
        // Wrong offsets shift every attribute (uv reading color produced
        // solid white glyph blocks with correct positions).
        assert_eq!(TEXT_VERT_FLOATS, 8);
        assert_eq!(TEXT_UV_OFFSET_BYTES, 8);
        assert_eq!(TEXT_COLOR_OFFSET_BYTES, 16);
        // glyph_quad emits exactly one 8-float vertex per 8 floats.
        let q = glyph_quad(0.0, 0.0, 7.0, 8.0, 0.0, 0.0, 7.0, 8.0, (1.0, 1.0, 1.0, 1.0));
        assert_eq!(q.len(), 48);
        assert_eq!(&q[0..4], &[0.0, 0.0, 0.0, 0.0]);
        assert_eq!(&q[4..8], &[1.0, 1.0, 1.0, 1.0]);
    }

    #[test]
    fn test_normalize_gray_padded_pitch() {
        use freetype::bitmap::PixelMode as PM;
        let src = [10u8, 20, 30, 0, 40, 50, 60, 0];
        let (w, h, data) = normalize_bitmap(3, 2, 4, PM::Gray, &src);
        assert_eq!((w, h), (3, 2));
        assert_eq!(data, vec![10, 20, 30, 40, 50, 60]);
    }

    #[test]
    fn test_fallback_resolves_missing_glyph() {
        let mut fonts = FontSystem::new().expect("freetype init");
        let ids = fonts
            .load_family_stack(&["monospace", "DejaVu Sans Mono", "Noto Sans Symbols"])
            .expect("stack");
        let primary = ids[0];
        // Plain ASCII comes from the primary face.
        let (fid_a, gid_a, _) = fonts
            .rasterize(primary, 'A', 15, SubpixelBucket(0))
            .expect("A");
        assert_eq!(fid_a, primary);
        assert_ne!(gid_a, 0);
        // U+276F (fish prompt) is missing from Noto Sans Mono: a fallback
        // face must supply it with a real glyph id and coverage.
        let (fid_sym, gid_sym, bm_sym) = fonts
            .rasterize(primary, '\u{276f}', 15, SubpixelBucket(0))
            .expect("fallback glyph");
        if ids.len() > 1 {
            assert_ne!(fid_sym, primary);
            assert_ne!(gid_sym, 0);
        }
        assert!(!bm_sym.data.is_empty());
    }

    #[test]
    fn test_normalize_negative_pitch_flips_rows() {
        use freetype::bitmap::PixelMode as PM;
        let src = [30u8, 40, 10, 20];
        let (w, h, data) = normalize_bitmap(2, 2, -2, PM::Gray, &src);
        assert_eq!((w, h), (2, 2));
        assert_eq!(data, vec![10, 20, 30, 40]);
    }

    #[test]
    fn test_normalize_mono_unpacks_bits() {
        use freetype::bitmap::PixelMode as PM;
        let src = [0b10100000u8];
        let (w, h, data) = normalize_bitmap(8, 1, 1, PM::Mono, &src);
        assert_eq!((w, h), (8, 1));
        assert_eq!(data, vec![255, 0, 255, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn test_normalize_lcd_averages_triples() {
        use freetype::bitmap::PixelMode as PM;
        let src = [10u8, 20, 30, 40, 50, 60];
        let (w, h, data) = normalize_bitmap(6, 1, 6, PM::Lcd, &src);
        assert_eq!((w, h), (2, 1));
        assert_eq!(data, vec![20, 50]);
    }

    #[test]
    fn test_normalize_bgra_takes_alpha() {
        use freetype::bitmap::PixelMode as PM;
        let src = [5u8, 6, 7, 128];
        let (w, h, data) = normalize_bitmap(1, 1, 4, PM::Bgra, &src);
        assert_eq!((w, h), (1, 1));
        assert_eq!(data, vec![128]);
    }

    #[test]
    fn test_normalize_empty_is_empty() {
        use freetype::bitmap::PixelMode as PM;
        let (w, h, data) = normalize_bitmap(0, 0, 0, PM::Gray, &[]);
        assert_eq!((w, h), (0, 0));
        assert!(data.is_empty());
    }

    #[test]
    fn test_font_metrics_cell_size() {
        let m = FontMetrics {
            ascender: 12.0,
            descender: -3.0,
            line_height: 17.6,
            max_advance: 8.2,
        };
        assert_eq!(m.cell_size(), (9.0, 18.0));
    }

    /// Headless raster repro: resolve "monospace" exactly like the app and
    /// dump real glyph geometry. Run with `-- --nocapture` to inspect.
    #[test]
    fn test_dump_glyph_geometry() {
        let mut fonts = FontSystem::new().expect("freetype init");
        let id = fonts.load_family("monospace").expect("monospace");
        let face = fonts.face_mut(id).unwrap();
        let m = face.font_metrics(15).expect("metrics");
        eprintln!("metrics15: {m:?} cell={:?}", m.cell_size());
        for ch in ['A', 'g', 'm', '~', '/', '@', '\u{276f}', '\u{2500}'] {
            let r = face.rasterize(ch, 15, SubpixelBucket(0));
            match r {
                None => eprintln!("U+{:04X} {ch:?}: MISSING", ch as u32),
                Some((gid, bm)) => {
                    eprintln!(
                        "U+{:04X} {ch:?}: gid={gid} {}x{} left={} top={} adv={:.1}",
                        ch as u32,
                        bm.width,
                        bm.height,
                        bm.left,
                        bm.top,
                        bm.advance as f64 / 64.0,
                    );
                    for y in 0..bm.height as usize {
                        let mut row = String::new();
                        for x in 0..bm.width as usize {
                            let v = bm.data[y * bm.width as usize + x];
                            row.push(if v > 200 {
                                '#'
                            } else if v > 100 {
                                '+'
                            } else if v > 25 {
                                '.'
                            } else {
                                ' '
                            });
                        }
                        eprintln!("  |{row}|");
                    }
                }
            }
        }
    }
}
