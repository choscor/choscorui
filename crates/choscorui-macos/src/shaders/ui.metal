// The shaders of the Metal painter. `scripts/macos/build-shaders.sh`
// compiles this file into `ui.metallib`; the crate embeds the library.
// Coordinates are device pixels with the origin at the top left. The
// layouts of `Instance`, `Stop`, `Clip`, `Edge`, and `BoxPass` match
// `crates/choscorui-macos/src/metal/scene.rs`.

#include <metal_stdlib>
using namespace metal;

constant uint KIND_FILL = 0;
constant uint KIND_STROKE = 1;
constant uint KIND_SHADOW = 2;
constant uint KIND_IMAGE = 3;
constant uint KIND_PATH = 4;
constant uint KIND_GLYPH = 5;
constant uint KIND_COLOR_GLYPH = 6;
constant uint KIND_BLUR = 7;
constant uint KIND_GROUP = 8;

constant uint FLAG_INSET = 1;
constant uint FLAG_NEAREST = 2;
constant uint FLAG_EVEN_ODD = 4;

constant uint PAINT_SOLID = 0;
constant uint PAINT_LINEAR = 1;

constant uint CLIP_SHAPE = 0;

// One primitive. The quad `bounds` is in local space; `xform` and
// `origin.xy` map local space to device pixels.
struct Instance {
    float4 bounds;
    float4 xform;
    // xy: the device translation; zw: the paint translation.
    float4 origin;
    // Maps local space to paint space, with `origin.zw`.
    float4 paint_xform;
    // Linear: start and end. Radial: center and radius.
    float4 gradient;
    float4 shape;
    // Corner radii clockwise from the top left.
    float4 radii;
    // Straight-alpha sRGB color.
    float4 color;
    float4 params;
    float4 params2;
    // The axis-aligned clip in device pixels.
    float4 clip;
    // Kind, flags, first clip, clip count.
    uint4 info;
    // Paint kind, first stop, stop count, coverage samples per pixel side.
    uint4 paint;
};

struct Stop {
    float4 color;
    float4 offset;
};

// One clip. A shape clip is a rounded rectangle in local space, which
// `inverse` and `offset` map from device pixels. A coverage clip is a
// region in device pixels whose winding numbers are in the coverage
// texture from `offset`, with `samples` per pixel side.
struct Clip {
    float4 rect;
    float4 radii;
    float4 inverse;
    float2 offset;
    uint kind;
    uint samples;
};

struct Fragment {
    float4 position [[position]];
    float2 local;
    uint instance [[flat]];
};

vertex Fragment ui_vertex(uint vertex_id [[vertex_id]],
                          uint instance_id [[instance_id]],
                          device const Instance *instances [[buffer(0)]],
                          constant float2 &viewport [[buffer(1)]]) {
    device const Instance &item = instances[instance_id];
    float2 corner = float2(float(vertex_id & 1), float(vertex_id >> 1));
    float2 local = mix(item.bounds.xy, item.bounds.zw, corner);
    float2 device_point = float2(
        item.xform.x * local.x + item.xform.z * local.y + item.origin.x,
        item.xform.y * local.x + item.xform.w * local.y + item.origin.y);
    Fragment out;
    out.position = float4(device_point.x / viewport.x * 2.0 - 1.0,
                          1.0 - device_point.y / viewport.y * 2.0, 0.0, 1.0);
    out.local = local;
    out.instance = instance_id;
    return out;
}

// Signed distance to a rectangle with one radius per corner, negative
// inside. It matches `rounded_rect_distance` in `choscorui-core`.
static float rounded_rect_distance(float2 p, float4 rect, float4 radii) {
    float2 center = (rect.xy + rect.zw) * 0.5;
    float2 half_size = (rect.zw - rect.xy) * 0.5;
    float2 q = p - center;
    float radius = q.x < 0.0 ? (q.y < 0.0 ? radii.x : radii.w)
                             : (q.y < 0.0 ? radii.y : radii.z);
    radius = clamp(radius, 0.0, max(min(half_size.x, half_size.y), 0.0));
    float2 d = abs(q) - half_size + radius;
    return length(max(d, 0.0)) + min(max(d.x, d.y), 0.0) - radius;
}

// The covered share of a pixel `pixel` units wide at signed distance
// `distance` from an edge.
static float coverage_of(float distance, float pixel) {
    return clamp(0.5 - distance / pixel, 0.0, 1.0);
}

// The Abramowitz and Stegun 7.1.26 error function, as in `choscorui-core`.
static float error_function(float x) {
    float sign_x = sign(x);
    x = abs(x);
    float t = 1.0 / (1.0 + 0.3275911 * x);
    float y = 1.0 - (((((1.061405429 * t - 1.453152027) * t) + 1.421413741) * t
                      - 0.284496736) * t + 0.254829592) * t * exp(-x * x);
    return sign_x * y;
}

static float4 premultiply(float4 color) {
    return float4(color.rgb * color.a, color.a);
}

// The paint at local point `p`. Gradients interpolate straight-alpha
// colors, as `Paint::color_at` in `choscorui-core` does.
static float4 paint_color(device const Instance &item, float2 p, device const Stop *stops) {
    if (item.paint.x == PAINT_SOLID) {
        return premultiply(item.color);
    }
    float2 q = float2(item.paint_xform.x * p.x + item.paint_xform.z * p.y + item.origin.z,
                      item.paint_xform.y * p.x + item.paint_xform.w * p.y + item.origin.w);
    float t;
    if (item.paint.x == PAINT_LINEAR) {
        float2 direction = item.gradient.zw - item.gradient.xy;
        float length_squared = dot(direction, direction);
        t = length_squared == 0.0 ? 0.0 : dot(q - item.gradient.xy, direction) / length_squared;
    } else {
        t = distance(q, item.gradient.xy) / item.gradient.z;
    }
    t = clamp(t, 0.0, 1.0);
    uint first = item.paint.y;
    uint count = item.paint.z;
    float4 color = stops[first].color;
    if (t > stops[first].offset.x) {
        color = stops[first + count - 1].color;
        for (uint k = 0; k + 1 < count; k++) {
            Stop a = stops[first + k];
            Stop b = stops[first + k + 1];
            if (t <= b.offset.x) {
                float span = max(b.offset.x - a.offset.x, 1e-12);
                color = round(mix(a.color, b.color, (t - a.offset.x) / span) * 255.0) / 255.0;
                break;
            }
        }
    }
    return premultiply(color);
}

// The share of the samples of the pixel at `pixel` that a path covers.
// The coverage texture holds one winding number per sample; the region
// starts at `origin` in device pixels and at `offset` in the texture.
static float sample_coverage(texture2d<float> coverage, float2 pixel, float2 origin,
                             float2 offset, uint samples, bool even_odd) {
    uint2 base = uint2((floor(pixel) - origin) * float(samples) + offset);
    uint covered = 0;
    for (uint y = 0; y < samples; y++) {
        for (uint x = 0; x < samples; x++) {
            int winding = int(round(coverage.read(base + uint2(x, y)).r));
            covered += even_odd ? uint(winding & 1) : uint(winding != 0);
        }
    }
    return float(covered) / float(samples * samples);
}

static bool inside(float2 p, float4 rect) {
    return p.x >= rect.x && p.y >= rect.y && p.x < rect.z && p.y < rect.w;
}

static float clip_coverage(device const Instance &item, float2 pixel_center,
                           device const Clip *clips, texture2d<float> coverage) {
    float4 c = item.clip;
    float x = clamp(min(pixel_center.x + 0.5, c.z) - max(pixel_center.x - 0.5, c.x), 0.0, 1.0);
    float y = clamp(min(pixel_center.y + 0.5, c.w) - max(pixel_center.y - 0.5, c.y), 0.0, 1.0);
    float result = x * y;
    for (uint k = 0; k < item.info.w && result > 0.0; k++) {
        Clip clip = clips[item.info.z + k];
        if (clip.kind == CLIP_SHAPE) {
            float2 local = float2(
                clip.inverse.x * pixel_center.x + clip.inverse.z * pixel_center.y + clip.offset.x,
                clip.inverse.y * pixel_center.x + clip.inverse.w * pixel_center.y + clip.offset.y);
            float pixel = 0.5 * (length(clip.inverse.xy) + length(clip.inverse.zw));
            result *= coverage_of(rounded_rect_distance(local, clip.rect, clip.radii), pixel);
        } else if (!inside(pixel_center, clip.rect)) {
            return 0.0;
        } else {
            result *= sample_coverage(coverage, pixel_center, clip.rect.xy, clip.offset,
                                      clip.samples, false);
        }
    }
    return result;
}

fragment float4 ui_fragment(Fragment in [[stage_in]],
                            device const Instance *instances [[buffer(0)]],
                            device const Stop *stops [[buffer(2)]],
                            device const Clip *clips [[buffer(3)]],
                            texture2d<float> source [[texture(0)]],
                            texture2d<float> coverage [[texture(1)]],
                            sampler linear_sampler [[sampler(0)]],
                            sampler nearest_sampler [[sampler(1)]]) {
    device const Instance &item = instances[in.instance];
    float2 p = in.local;
    float2 step = fwidth(p);
    float pixel = max(0.5 * (step.x + step.y), 1e-6);
    float2 center = in.position.xy;
    float alpha = clip_coverage(item, center, clips, coverage);
    if (alpha <= 0.0) {
        discard_fragment();
    }
    uint kind = item.info.x;
    float4 color = float4(0.0);
    if (kind == KIND_FILL) {
        alpha *= coverage_of(rounded_rect_distance(p, item.shape, item.radii), pixel);
        color = paint_color(item, p, stops);
    } else if (kind == KIND_STROKE) {
        float outer = coverage_of(rounded_rect_distance(p, item.shape, item.radii), pixel);
        float inner = coverage_of(rounded_rect_distance(p, item.params2, item.params), pixel);
        alpha *= clamp(outer - inner, 0.0, 1.0);
        color = paint_color(item, p, stops);
    } else if (kind == KIND_SHADOW) {
        bool inset = (item.info.y & FLAG_INSET) != 0;
        bool in_caster = rounded_rect_distance(p, item.shape, item.radii) < 0.0;
        float d = rounded_rect_distance(p, item.params2, item.params) / item.origin.z;
        float shadow = 0.0;
        if (!inset && !in_caster) {
            shadow = 0.5 * (1.0 - error_function(d));
        } else if (inset && in_caster) {
            shadow = 0.5 * (1.0 + error_function(d));
        }
        // The core mask rounds the coverage to an 8-bit alpha and covers
        // the shadow bounds in `gradient`.
        float shadow_alpha = round(shadow * item.color.a * 255.0) / 255.0;
        alpha *= coverage_of(rounded_rect_distance(p, item.gradient, float4(0.0)), pixel);
        color = float4(item.color.rgb * shadow_alpha, shadow_alpha);
    } else if (kind == KIND_IMAGE) {
        alpha *= coverage_of(rounded_rect_distance(p, item.shape, float4(0.0)), pixel);
        float2 uv = (p - item.shape.xy) / (item.shape.zw - item.shape.xy);
        bool nearest = (item.info.y & FLAG_NEAREST) != 0;
        color = nearest ? source.sample(nearest_sampler, uv, level(0.0))
                        : source.sample(linear_sampler, uv, level(0.0));
    } else if (kind == KIND_PATH) {
        if (!inside(center, item.shape)) {
            discard_fragment();
        }
        bool even_odd = (item.info.y & FLAG_EVEN_ODD) != 0;
        alpha *= sample_coverage(coverage, center, item.shape.xy, item.params.xy, item.paint.w,
                                 even_odd);
        color = paint_color(item, p, stops);
    } else if (kind == KIND_GLYPH || kind == KIND_COLOR_GLYPH) {
        float2 size = float2(source.get_width(), source.get_height());
        float2 t = (p - item.shape.xy) / (item.shape.zw - item.shape.xy);
        float2 uv = mix(item.params.xy, item.params.zw, t) / size;
        float4 texel = source.sample(linear_sampler, uv, level(0.0));
        if (kind == KIND_GLYPH) {
            color = premultiply(item.color) * texel.r;
        } else {
            color = texel * item.color.a;
        }
    } else if (kind == KIND_BLUR) {
        alpha *= coverage_of(rounded_rect_distance(p, item.shape, item.radii), pixel);
        float2 texel = floor(center) + item.params.xy;
        float2 limit = float2(source.get_width(), source.get_height()) - 1.0;
        color = source.read(uint2(clamp(texel, float2(0.0), limit)));
    } else if (kind == KIND_GROUP) {
        color = source.read(uint2(center)) * item.params.x;
    }
    return color * alpha;
}

// One flattened path edge in coverage samples, and the quad of samples
// that it can change: from the left side of the region to the edge.
struct Edge {
    float4 line;
    float4 quad;
};

struct EdgeFragment {
    float4 position [[position]];
    uint edge [[flat]];
};

vertex EdgeFragment edge_vertex(uint vertex_id [[vertex_id]],
                                uint instance_id [[instance_id]],
                                device const Edge *edges [[buffer(0)]],
                                constant float2 &size [[buffer(1)]]) {
    Edge edge = edges[instance_id];
    float2 corner = float2(float(vertex_id & 1), float(vertex_id >> 1));
    float2 p = mix(edge.quad.xy, edge.quad.zw, corner);
    EdgeFragment out;
    out.position = float4(p.x / size.x * 2.0 - 1.0, 1.0 - p.y / size.y * 2.0, 0.0, 1.0);
    out.edge = instance_id;
    return out;
}

// The winding number that the edge adds at the sample: a ray from the
// sample to the right crosses it. The rule matches `Path::contains`.
fragment float edge_fragment(EdgeFragment in [[stage_in]],
                             device const Edge *edges [[buffer(0)]]) {
    float2 p = in.position.xy;
    Edge edge = edges[in.edge];
    float2 a = edge.line.xy;
    float2 b = edge.line.zw;
    float cross_value = (b.x - a.x) * (p.y - a.y) - (p.x - a.x) * (b.y - a.y);
    if (a.y <= p.y) {
        if (b.y > p.y && cross_value > 0.0) {
            return 1.0;
        }
    } else if (b.y <= p.y && cross_value < 0.0) {
        return -1.0;
    }
    discard_fragment();
    return 0.0;
}

struct Screen {
    float4 position [[position]];
};

// One triangle that covers the whole target.
vertex Screen screen_vertex(uint vertex_id [[vertex_id]]) {
    float2 p = float2(float((vertex_id << 1) & 2), float(vertex_id & 2));
    Screen out;
    out.position = float4(p * 2.0 - 1.0, 0.0, 1.0);
    return out;
}

fragment float4 copy_fragment(Screen in [[stage_in]], texture2d<float> source [[texture(0)]]) {
    return source.read(uint2(in.position.xy));
}

// One box blur pass along `direction`, with the integer rounding of
// `blur_pixels` in `choscorui-core`. Edges repeat the nearest pixel.
struct BoxPass {
    int2 direction;
    int radius;
    int pad;
};

fragment float4 box_fragment(Screen in [[stage_in]],
                             texture2d<float> source [[texture(0)]],
                             constant BoxPass &pass [[buffer(0)]]) {
    int2 p = int2(in.position.xy);
    int2 limit = int2(source.get_width(), source.get_height()) - 1;
    uint4 sum = uint4(0);
    for (int k = -pass.radius; k <= pass.radius; k++) {
        int2 q = clamp(p + pass.direction * k, int2(0), limit);
        sum += uint4(round(source.read(uint2(q)) * 255.0));
    }
    uint span = uint(2 * pass.radius + 1);
    return float4((sum + span / 2) / span) / 255.0;
}
