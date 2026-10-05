#version 330
#extension GL_ARB_separate_shader_objects : require

// Car parts whose material is not ported (trim pieces, eyes): the extracted base colour lit by a
// wrapped diffuse and a soft highlight, in the same light as the ported car materials
// (rlcar:rl_env.glsl), so they sit with the rest of the car.

#include <rlcar:rl_env.glsl>

uniform sampler2D BaseMap; // sRGB

void main() {
    RlEnv e = rlEnv();
    vec4 base = texture(BaseMap, texCoord0);
    if (base.a < 0.1) {
        discard;
    }
    vec3 albedo = srgbToLinear(base.rgb);
    float nl = dot(e.N, e.L) * 0.5 + 0.5;
    vec3 h = normalize(e.L + e.V);
    vec3 color = albedo * (shIrradiance(e, e.N) + skyUpper(e) + e.lightColor * nl * nl);
    color += e.lightColor * pow(clamp(dot(e.N, h), 0.0, 1.0), 40.0) * 0.15;
    color += envCube(e, reflect(-e.V, e.N)) * 0.04;
    fragColor = rlFinish(color);
}
