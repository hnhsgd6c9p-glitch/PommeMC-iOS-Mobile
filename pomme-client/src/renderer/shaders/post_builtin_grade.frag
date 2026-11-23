#version 450

// Mobile-friendly cinematic grade with a small bright-pass bloom approximation.
// Five texture reads total; no extra render target or full-resolution blur chain.
layout(set = 0, binding = 0) uniform texture2D in_tex;
layout(set = 0, binding = 1) uniform sampler in_tex_sampler;
layout(push_constant) uniform PushConstants {
    vec4 controls; // saturation, contrast, warmth, vignette
};
layout(location = 0) in vec2 v_uv;
layout(location = 0) out vec4 out_color;

void main() {
    vec3 color = texture(sampler2D(in_tex, in_tex_sampler), v_uv).rgb;
    vec2 texel = 1.0 / vec2(textureSize(sampler2D(in_tex, in_tex_sampler), 0));

    // A low-cost highlight glow, gated to bright neighboring pixels.
    vec3 bloom = vec3(0.0);
    bloom += max(
        texture(sampler2D(in_tex, in_tex_sampler), v_uv + vec2(texel.x, 0.0)).rgb - vec3(0.72),
        vec3(0.0)
    );
    bloom += max(
        texture(sampler2D(in_tex, in_tex_sampler), v_uv - vec2(texel.x, 0.0)).rgb - vec3(0.72),
        vec3(0.0)
    );
    bloom += max(
        texture(sampler2D(in_tex, in_tex_sampler), v_uv + vec2(0.0, texel.y)).rgb - vec3(0.72),
        vec3(0.0)
    );
    bloom += max(
        texture(sampler2D(in_tex, in_tex_sampler), v_uv - vec2(0.0, texel.y)).rgb - vec3(0.72),
        vec3(0.0)
    );
    bloom *= 0.07 + abs(controls.z) * 0.18 + max(controls.x - 1.0, 0.0) * 0.035;

    float luma = dot(color, vec3(0.2126, 0.7152, 0.0722));
    color = mix(vec3(luma), color, controls.x);
    color = (color - 0.5) * controls.y + 0.5;
    color += vec3(controls.z * 0.55, controls.z * 0.12, -controls.z * 0.45);
    color += bloom;

    // Gentle filmic shoulder preserves detail in bright skies and lava.
    color = color / (vec3(1.0) + max(color, vec3(0.0)) * 0.12);
    vec2 centered = v_uv * 2.0 - 1.0;
    float edge = dot(centered, centered);
    color *= 1.0 - controls.w * smoothstep(0.25, 1.8, edge);
    out_color = vec4(clamp(color, 0.0, 1.0), 1.0);
}
