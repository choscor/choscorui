//! The GLSL ES 3.00 source of the painter's one program. The driver
//! compiles it when the window system starts.
//!
//! Each draw is a quad (or the quads of a glyph run) in device pixels,
//! with y down from the top edge. The fragment shader multiplies three
//! factors:
//!
//! - the paint: a solid color, a gradient ramp, an image, or a layer;
//! - the coverage: the whole quad, an exact rectangle, a Cairo mask, or a
//!   glyph in the atlas;
//! - the clip: the coverage texture of the innermost masked clip.
//!
//! Colors are premultiplied, and the blend is source-over.

/// The vertex shader: device pixels to clip space.
pub(super) const VERTEX: &str = "#version 300 es
uniform vec2 u_target;
in vec2 a_position;
in vec2 a_uv;
out vec2 v_uv;
void main() {
    v_uv = a_uv;
    vec2 unit = a_position / u_target;
    gl_Position = vec4(unit.x * 2.0 - 1.0, 1.0 - unit.y * 2.0, 0.0, 1.0);
}
";

/// The fragment shader.
pub(super) const FRAGMENT: &str = "#version 300 es
precision highp float;
precision highp int;

// The size of the target in pixels.
uniform vec2 u_target;
// 0 solid, 1 linear gradient, 2 radial gradient, 3 image, 4 layer.
uniform int u_paint;
// The premultiplied solid color; its alpha is the opacity of 3 and 4.
uniform vec4 u_color;
// Device pixels to the user space of a gradient or an image.
uniform mat3 u_inverse;
// 1: start, end. 2: center, radius. 3: the image rectangle.
uniform vec4 u_geometry;
// The ramp of 1 and 2, the image of 3, the layer of 4.
uniform sampler2D u_source;
// 0 whole quad, 1 rectangle, 2 mask, 3 atlas.
uniform int u_coverage;
// 1: device x0, y0, x1, y1. 2: mask x, y, width, height.
uniform vec4 u_rect;
// The mask of 2, the atlas of 3.
uniform sampler2D u_mask;
uniform vec2 u_atlas;
// 1 when the clip has a coverage texture.
uniform int u_clip;
uniform sampler2D u_clip_mask;

in vec2 v_uv;
out vec4 o_color;

// The length of [lo, hi] inside [a, b].
float overlap(float lo, float hi, float a, float b) {
    return clamp(min(hi, b) - max(lo, a), 0.0, 1.0);
}

void main() {
    vec2 device = vec2(gl_FragCoord.x, u_target.y - gl_FragCoord.y);
    float coverage = 1.0;
    if (u_coverage == 1) {
        coverage = overlap(device.x - 0.5, device.x + 0.5, u_rect.x, u_rect.z)
            * overlap(device.y - 0.5, device.y + 0.5, u_rect.y, u_rect.w);
    } else if (u_coverage == 2) {
        coverage = texture(u_mask, (device - u_rect.xy) / u_rect.zw).r;
    } else if (u_coverage == 3) {
        coverage = texture(u_mask, v_uv / u_atlas).r;
    }
    if (u_clip == 1) {
        coverage *= texture(u_clip_mask, gl_FragCoord.xy / u_target).r;
    }
    vec4 color = u_color;
    if (u_paint == 1 || u_paint == 2) {
        vec2 point = (u_inverse * vec3(device, 1.0)).xy;
        float t;
        if (u_paint == 1) {
            vec2 axis = u_geometry.zw - u_geometry.xy;
            float length2 = dot(axis, axis);
            t = length2 == 0.0 ? 0.0 : dot(point - u_geometry.xy, axis) / length2;
        } else {
            t = u_geometry.z == 0.0 ? 1.0 : length(point - u_geometry.xy) / u_geometry.z;
        }
        t = clamp(t, 0.0, 1.0);
        color = texture(u_source, vec2((t * 255.0 + 0.5) / 256.0, 0.5));
    } else if (u_paint == 3) {
        vec2 point = (u_inverse * vec3(device, 1.0)).xy;
        color = texture(u_source, (point - u_geometry.xy) / u_geometry.zw) * u_color.a;
    } else if (u_paint == 4) {
        color = texture(u_source, gl_FragCoord.xy / u_target) * u_color.a;
    }
    o_color = color * coverage;
}
";
