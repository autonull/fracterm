//! Canvas core — OpenGL 3.3 core context, capability detection, and the
//! batched rectangle renderer for the workspace (README M1).
//!
//! The renderer draws every visible node as a batched quad in world space,
//! transformed by the workspace camera. Rendering is structured around the
//! render graph stages (`ContentPass` → `OverlayPass` → `PostProcessPass`);
//! post-process effects degrade gracefully on weak drivers.

use glow::HasContext;

/// Driver capabilities detected at startup.
/// Used to degrade effects gracefully (no compute required).
#[derive(Debug, Clone)]
pub struct Capabilities {
    pub gl_version: String,
    pub renderer: String,
    pub fbo_supported: bool,
    pub post_process_supported: bool,
}

impl Capabilities {
    /// Detect capabilities from the current GL context.
    pub fn detect(gl: &glow::Context) -> Self {
        unsafe {
            let version = gl.get_parameter_string(glow::VERSION);
            let renderer = gl.get_parameter_string(glow::RENDERER);
            let major_ok = version.starts_with("3.3")
                || version.starts_with("4.")
                || version.starts_with("OpenGL 3.3")
                || version.starts_with("OpenGL 4.");
            Self {
                gl_version: version,
                renderer,
                fbo_supported: major_ok,
                post_process_supported: major_ok,
            }
        }
    }
}

const VERTEX_SRC: &str = r#"
#version 330 core
layout (location = 0) in vec2 a_pos;
layout (location = 1) in vec4 a_color;
uniform vec2 u_cam_pos;
uniform float u_zoom;
uniform vec2 u_viewport;
out vec4 v_color;
void main() {
    vec2 screen = (a_pos - u_cam_pos) * u_zoom;
    vec2 ndc = vec2(2.0 * screen.x / u_viewport.x - 1.0, 1.0 - 2.0 * screen.y / u_viewport.y);
    gl_Position = vec4(ndc, 0.0, 1.0);
    v_color = a_color;
}
"#;

const FRAGMENT_SRC: &str = r#"
#version 330 core
in vec4 v_color;
out vec4 frag_color;
void main() {
    frag_color = v_color;
}
"#;

/// Batched rectangle renderer: one draw call per frame for all node rects.
pub struct RectRenderer {
    program: glow::Program,
    vbo: glow::Buffer,
    vao: glow::VertexArray,
    verts: Vec<f32>,
    overlay_verts: Vec<f32>,
    cam_pos: (f32, f32),
    zoom: f32,
    viewport: (f32, f32),
    clear_color: (f32, f32, f32, f32),
}

impl RectRenderer {
    /// Create the renderer. A single VAO/VBO pair is used for all batches.
    ///
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
        gl.attach_shader(program, shader(glow::VERTEX_SHADER, VERTEX_SRC)?);
        gl.attach_shader(program, shader(glow::FRAGMENT_SHADER, FRAGMENT_SRC)?);
        gl.link_program(program);
        if !gl.get_program_link_status(program) {
            return Err(gl.get_program_info_log(program));
        }

        let vao = gl.create_vertex_array()?;
        let vbo = gl.create_buffer()?;
        gl.bind_vertex_array(Some(vao));
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
        // 6 verts * (pos 2 + color 4) floats
        let stride = 6 * std::mem::size_of::<f32>() as i32;
        gl.enable_vertex_attrib_array(0);
        gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, stride, 0);
        gl.enable_vertex_attrib_array(1);
        gl.vertex_attrib_pointer_f32(
            1,
            4,
            glow::FLOAT,
            false,
            stride,
            2 * std::mem::size_of::<f32>() as i32,
        );
        gl.bind_vertex_array(None);

        Ok(Self {
            program,
            vbo,
            vao,
            verts: Vec::new(),
            overlay_verts: Vec::new(),
            cam_pos: (0.0, 0.0),
            zoom: 1.0,
            viewport: (1280.0, 720.0),
            clear_color: (0.043, 0.051, 0.071, 1.0),
        })
    }

    pub fn set_camera(&mut self, cam_x: f64, cam_y: f64, zoom: f64) {
        self.cam_pos = (cam_x as f32, cam_y as f32);
        self.zoom = zoom as f32;
    }

    pub fn set_viewport(&mut self, width: f32, height: f32) {
        self.viewport = (width, height);
    }

    pub fn set_clear_color(&mut self, c: (f32, f32, f32, f32)) {
        self.clear_color = c;
    }

    /// Queue a world-space rectangle outline (4 thin rects) for the OverlayPass.
    pub fn push_border(
        &mut self,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        thickness: f64,
        color: (f32, f32, f32, f32),
    ) {
        let v = &mut self.overlay_verts;
        push_rect_verts(v, x, y, w, thickness, color);
        push_rect_verts(v, x, y + h - thickness, w, thickness, color);
        push_rect_verts(v, x, y, thickness, h, color);
        push_rect_verts(v, x + w - thickness, y, thickness, h, color);
    }

    /// Queue a filled world-space rectangle into the OverlayPass.
    pub fn push_overlay_rect(
        &mut self,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        color: (f32, f32, f32, f32),
    ) {
        push_rect_verts(&mut self.overlay_verts, x, y, w, h, color);
    }

    /// Queue an axis-aligned world-space rectangle (6 vertices, 2 triangles).
    pub fn push_rect(&mut self, x: f64, y: f64, w: f64, h: f64, color: (f32, f32, f32, f32)) {
        push_rect_verts(&mut self.verts, x, y, w, h, color);
    }

    /// Draw all queued rects in a single draw call (ContentPass).
    ///
    /// # Safety
    /// `gl` must be the same current context the renderer was created with.
    pub unsafe fn draw(&mut self, gl: &glow::Context) {
        gl.viewport(0, 0, self.viewport.0 as i32, self.viewport.1 as i32);
        gl.clear_color(
            self.clear_color.0,
            self.clear_color.1,
            self.clear_color.2,
            self.clear_color.3,
        );
        gl.clear(glow::COLOR_BUFFER_BIT);
        gl.enable(glow::BLEND);
        gl.blend_func_separate(
            glow::SRC_ALPHA,
            glow::ONE_MINUS_SRC_ALPHA,
            glow::ONE,
            glow::ONE_MINUS_SRC_ALPHA,
        );

        if self.verts.is_empty() {
            return;
        }
        self.upload_and_draw(gl, &self.verts);
        self.verts.clear();
    }

    /// Draw all queued overlay rects (OverlayPass: borders, handles, selection).
    ///
    /// # Safety
    /// `gl` must be the same current context the renderer was created with.
    pub unsafe fn draw_overlay(&mut self, gl: &glow::Context) {
        gl.viewport(0, 0, self.viewport.0 as i32, self.viewport.1 as i32);
        gl.clear_color(0.0, 0.0, 0.0, 0.0);
        gl.clear(glow::COLOR_BUFFER_BIT);
        gl.enable(glow::BLEND);
        gl.blend_func_separate(
            glow::SRC_ALPHA,
            glow::ONE_MINUS_SRC_ALPHA,
            glow::ONE,
            glow::ONE_MINUS_SRC_ALPHA,
        );

        if self.overlay_verts.is_empty() {
            return;
        }
        self.upload_and_draw(gl, &self.overlay_verts);
        self.overlay_verts.clear();
    }

    unsafe fn upload_and_draw(&self, gl: &glow::Context, verts: &[f32]) {
        gl.use_program(Some(self.program));
        gl.uniform_2_f32(
            gl.get_uniform_location(self.program, "u_cam_pos").as_ref(),
            self.cam_pos.0,
            self.cam_pos.1,
        );
        gl.uniform_1_f32(
            gl.get_uniform_location(self.program, "u_zoom").as_ref(),
            self.zoom,
        );
        gl.uniform_2_f32(
            gl.get_uniform_location(self.program, "u_viewport").as_ref(),
            self.viewport.0,
            self.viewport.1,
        );

        gl.bind_vertex_array(Some(self.vao));
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vbo));
        gl.buffer_data_u8_slice(
            glow::ARRAY_BUFFER,
            bytemuck_bytes(verts),
            glow::DYNAMIC_DRAW,
        );
        gl.draw_arrays(glow::TRIANGLES, 0, (verts.len() / 6) as i32);
        gl.bind_vertex_array(None);
    }
}

fn push_rect_verts(
    out: &mut Vec<f32>,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    color: (f32, f32, f32, f32),
) {
    let (x0, y0) = (x as f32, y as f32);
    let (x1, y1) = ((x + w) as f32, (y + h) as f32);
    let (r, g, b, a) = color;
    let quad: [[f32; 6]; 6] = [
        [x0, y0, r, g, b, a],
        [x1, y0, r, g, b, a],
        [x0, y1, r, g, b, a],
        [x1, y0, r, g, b, a],
        [x1, y1, r, g, b, a],
        [x0, y1, r, g, b, a],
    ];
    for v in quad {
        out.extend_from_slice(&v);
    }
}

/// Reinterpret an f32 slice as bytes without an extra dependency.
fn bytemuck_bytes(slice: &[f32]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(slice.as_ptr().cast(), std::mem::size_of_val(slice)) }
}

/// A framebuffer render target (FBO + color texture) used by the render graph.
pub struct RenderTarget {
    fbo: glow::Framebuffer,
    texture: glow::Texture,
    width: u32,
    height: u32,
}

impl RenderTarget {
    /// Create an empty target; call `resize` before first use.
    ///
    /// # Safety
    /// `gl` must be a valid, current OpenGL context.
    pub unsafe fn new(gl: &glow::Context) -> Self {
        Self {
            fbo: gl.create_framebuffer().expect("failed to create FBO"),
            texture: gl.create_texture().expect("failed to create texture"),
            width: 0,
            height: 0,
        }
    }

    /// (Re)allocate the color texture to the given size.
    ///
    /// # Safety
    /// `gl` must be a valid, current OpenGL context.
    pub unsafe fn resize(&mut self, gl: &glow::Context, width: u32, height: u32) {
        if self.width == width && self.height == height {
            return;
        }
        self.width = width;
        self.height = height;
        gl.bind_texture(glow::TEXTURE_2D, Some(self.texture));
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::RGBA as i32,
            width as i32,
            height as i32,
            0,
            glow::RGBA,
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
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.fbo));
        gl.framebuffer_texture_2d(
            glow::FRAMEBUFFER,
            glow::COLOR_ATTACHMENT0,
            glow::TEXTURE_2D,
            Some(self.texture),
            0,
        );
        gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        gl.bind_texture(glow::TEXTURE_2D, None);
    }

    /// Bind this target for rendering.
    ///
    /// # Safety
    /// `gl` must be a valid, current OpenGL context.
    pub unsafe fn bind(&self, gl: &glow::Context) {
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.fbo));
        gl.viewport(0, 0, self.width as i32, self.height as i32);
    }

    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }
}

const TEXTURE_VERTEX_SRC: &str = r#"
#version 330 core
layout (location = 0) in vec2 a_pos;
layout (location = 1) in vec2 a_uv;
out vec2 v_uv;
void main() {
    gl_Position = vec4(a_pos, 0.0, 1.0);
    v_uv = a_uv;
}
"#;

const TEXTURE_FRAGMENT_SRC: &str = r#"
#version 330 core
in vec2 v_uv;
uniform sampler2D u_texture;
out vec4 frag_color;
void main() {
    frag_color = texture(u_texture, v_uv);
}
"#;

/// FBO-backed executor for the render graph:
/// `ContentPass` -> `OverlayPass` -> `PostProcessPass` -> screen.
pub struct RenderGraphExecutor {
    content: RenderTarget,
    overlay: RenderTarget,
    post: RenderTarget,
    composite_program: glow::Program,
    quad_vao: glow::VertexArray,
    /// When false (weak drivers), passes render directly to the screen.
    pub fbo_enabled: bool,
}

/// Which graph target to draw into / sample from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PassTarget {
    Content,
    Overlay,
    Post,
}

impl RenderGraphExecutor {
    /// Create the executor. `fbo_enabled` should come from `Capabilities`.
    ///
    /// # Safety
    /// `gl` must be a valid, current OpenGL context.
    pub unsafe fn new(gl: &glow::Context, fbo_enabled: bool) -> Result<Self, String> {
        let content = RenderTarget::new(gl);
        let overlay = RenderTarget::new(gl);
        let post = RenderTarget::new(gl);

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
        gl.attach_shader(program, shader(glow::VERTEX_SHADER, TEXTURE_VERTEX_SRC)?);
        gl.attach_shader(
            program,
            shader(glow::FRAGMENT_SHADER, TEXTURE_FRAGMENT_SRC)?,
        );
        gl.link_program(program);
        if !gl.get_program_link_status(program) {
            return Err(gl.get_program_info_log(program));
        }

        let vao = gl.create_vertex_array()?;
        let vbo = gl.create_buffer()?;
        gl.bind_vertex_array(Some(vao));
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
        // fullscreen triangle strip quad: pos(2) + uv(2)
        let quad: [f32; 16] = [
            -1.0, -1.0, 0.0, 1.0, 1.0, -1.0, 1.0, 1.0, -1.0, 1.0, 0.0, 0.0, 1.0, 1.0, 1.0, 0.0,
        ];
        gl.buffer_data_u8_slice(
            glow::ARRAY_BUFFER,
            std::slice::from_raw_parts(
                quad.as_ptr().cast(),
                quad.len() * std::mem::size_of::<f32>(),
            ),
            glow::STATIC_DRAW,
        );
        gl.enable_vertex_attrib_array(0);
        gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, 16, 0);
        gl.enable_vertex_attrib_array(1);
        gl.vertex_attrib_pointer_f32(1, 2, glow::FLOAT, false, 16, 8);
        gl.bind_vertex_array(None);
        gl.bind_buffer(glow::ARRAY_BUFFER, None);

        let _ = vbo; // buffer is bound to the VAO and never needed again
        Ok(Self {
            content,
            overlay,
            post,
            composite_program: program,
            quad_vao: vao,
            fbo_enabled,
        })
    }

    /// Resize all targets. Call on window resize.
    ///
    /// # Safety
    /// `gl` must be a valid, current OpenGL context.
    pub unsafe fn resize(&mut self, gl: &glow::Context, width: u32, height: u32) {
        self.content.resize(gl, width, height);
        self.overlay.resize(gl, width, height);
        self.post.resize(gl, width, height);
    }

    /// Begin a pass: bind its target (or nothing for direct screen rendering).
    ///
    /// # Safety
    /// `gl` must be a valid, current OpenGL context.
    pub unsafe fn begin_pass(&self, gl: &glow::Context, target: PassTarget) {
        if !self.fbo_enabled {
            return;
        }
        match target {
            PassTarget::Content => self.content.bind(gl),
            PassTarget::Overlay => self.overlay.bind(gl),
            PassTarget::Post => self.post.bind(gl),
        }
    }

    /// Blit `src` into `dst` via a fullscreen textured quad.
    ///
    /// # Safety
    /// `gl` must be a valid, current OpenGL context.
    unsafe fn blit(&self, gl: &glow::Context, src: &RenderTarget) {
        gl.use_program(Some(self.composite_program));
        gl.active_texture(glow::TEXTURE0);
        gl.bind_texture(glow::TEXTURE_2D, Some(src.texture));
        gl.uniform_1_i32(
            gl.get_uniform_location(self.composite_program, "u_texture")
                .as_ref(),
            0,
        );
        gl.bind_vertex_array(Some(self.quad_vao));
        gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
        gl.bind_vertex_array(None);
    }

    /// Finish a pass by compositing it over the accumulated post target.
    ///
    /// # Safety
    /// `gl` must be a valid, current OpenGL context.
    pub unsafe fn end_pass(&self, gl: &glow::Context, source: PassTarget) {
        if !self.fbo_enabled {
            return;
        }
        self.begin_pass(gl, PassTarget::Post);
        gl.enable(glow::BLEND);
        gl.blend_func_separate(
            glow::SRC_ALPHA,
            glow::ONE_MINUS_SRC_ALPHA,
            glow::ONE,
            glow::ONE_MINUS_SRC_ALPHA,
        );
        match source {
            PassTarget::Content => self.blit(gl, &self.content),
            PassTarget::Overlay => self.blit(gl, &self.overlay),
            PassTarget::Post => {}
        }
    }

    /// Present the post target to the default framebuffer.
    ///
    /// # Safety
    /// `gl` must be a valid, current OpenGL context.
    pub unsafe fn present(&self, gl: &glow::Context) {
        if self.fbo_enabled {
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            gl.disable(glow::BLEND);
            self.blit(gl, &self.post);
        }
        // Direct-screen mode already wrote to the default framebuffer.
    }
}
