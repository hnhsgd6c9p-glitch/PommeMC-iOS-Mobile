#version 450

// Pomme's inexpensive mobile color-grade pass. One texture sample per pixel.
layout(set = 0, binding = 0) uniform texture2D in_tex;
layout(set = 0, binding = 1) uniform sampler in_tex_sampler;
layout(push_constant) uniform PushConstants {
    vec4 controls; // saturation, contrast, warmth, vignette
};
layout(location = 0) in vec2 v_uv;
layout(location = 0) out vec4 out_color;

void main() {
    vec3 color = texture(sampler2D(in_tex, in_tex_sampler), v_uv).rgb;
    float luma = dot(color, vec3(0.2126, 0.7152, 0.0722));
    color = mix(vec3(luma), color, controls.x);
    color = (color - 0.5) * controls.y + 0.5;
    color += vec3(controls.z * 0.55, controls.z * 0.12, -controls.z * 0.45);
    vec2 centered = v_uv * 2.0 - 1.0;
    float edge = dot(centered, centered);
    color *= 1.0 - controls.w * smoothstep(0.25, 1.8, edge);
    out_color = vec4(clamp(color, 0.0, 1.0), 1.0);
}
