#include <metal_stdlib>
using namespace metal;

// Neko decorative surface shaders. Compiled into Neko.metallib by scripts/build-native.sh.

/// Opaque nacre: broad softbox lighting, a translucent-looking coloured body and
/// restrained interference bands. This shades the bead only, not system glass.
/// Pointer uniforms move the light; no clock or continuously running animation.
[[stitchable]] half4 pearlOrb(float2 pos, half4 source, float2 size, float2 pointer,
                            float hover, half4 tint, half4 sheen, half4 illuminant) {
    if (source.a <= 0.0h) return source;
    float2 p = (pos - size * 0.5) / max(1.0, min(size.x, size.y) * 0.5);
    float r2 = min(dot(p, p), 1.0);
    float3 n = float3(p, sqrt(max(0.0, 1.0 - r2)));
    float3 light = normalize(float3(-0.45 + pointer.x * 0.48, -0.6 + pointer.y * 0.42, 1.25));
    float diffuse = max(0.0, dot(n, light));
    float grazing = pow(1.0 - n.z, 2.2);
    float3 white = float3(illuminant.rgb);
    float3 base = float3(tint.rgb);
    // Soft body transmission prevents the near-black rim of a metal ball.
    float3 col = mix(base, white, 0.36) * (0.70 + diffuse * 0.22);
    float layers = sin(n.x * 5.2 + n.y * 3.5 + n.z * 6.0 + pointer.x * 0.45);
    float veil = smoothstep(-0.8, 0.9, layers) * (0.11 + grazing * 0.13);
    col = mix(col, float3(sheen.rgb), veil);
    // Curved nacre bands live inside the sphere; broad highlights avoid chrome pinpoints.
    float band = exp(-pow((n.y + n.x * 0.35 - 0.32 + pointer.y * 0.12) * 5.0, 2.0));
    col += float3(sheen.rgb) * band * 0.08 * (0.3 + n.z);
    float3 halfway = normalize(light + float3(0, 0, 1));
    float softbox = pow(max(0.0, dot(n, halfway)), 12.0);
    float rim = grazing * (0.065 + 0.045 * max(0.0, -n.x));
    col = mix(col, white, softbox * (0.48 + hover * 0.10));
    col += white * rim;
    return half4(half3(clamp(col, 0.0, 1.0)) * source.a, source.a);
}

static float hash21(float2 p) { return fract(sin(dot(p, float2(127.1, 311.7))) * 43758.5453); }
static float vnoise(float2 p) {
    float2 i = floor(p), f = fract(p);
    float2 u = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash21(i), hash21(i + float2(1, 0)), u.x),
               mix(hash21(i + float2(0, 1)), hash21(i + float2(1, 1)), u.x), u.y);
}
static float fbm(float2 p) {
    float v = 0.0, a = 0.5;
    for (int k = 0; k < 5; k++) { v += a * vnoise(p); p = p * 2.03 + float2(1.7, 9.2); a *= 0.5; }
    return v;
}
// Jewel palette: ink -> sapphire -> amethyst -> a rare teal glint. No browns.
static float3 jewel(float t) {
    t = clamp(t, 0.0, 1.0);
    const float3 ink = float3(0.020, 0.024, 0.070);
    const float3 sapphire = float3(0.070, 0.110, 0.360);
    const float3 amethyst = float3(0.260, 0.090, 0.420);
    const float3 teal = float3(0.040, 0.300, 0.360);
    float3 c = mix(ink, sapphire, smoothstep(0.10, 0.45, t));
    c = mix(c, amethyst, smoothstep(0.45, 0.75, t));
    return mix(c, teal, smoothstep(0.82, 1.0, t) * 0.6);
}

/// Full-window background: domain-warped noise gives slow, liquid facets of
/// colour; thin ridges read as caustic light moving through a stone.
[[stitchable]] half4 gemField(float2 pos, half4 color, float2 size, float time) {
    float2 uv = pos / size;
    float2 p = (pos - size * 0.5) / min(size.x, size.y);
    float2 q = float2(fbm(p * 1.5 + time * 0.035), fbm(p * 1.5 + float2(5.2, 1.3) - time * 0.03));
    float2 r = float2(fbm(p * 2.1 + 3.0 * q + float2(1.7, 9.2) + time * 0.045), fbm(p * 2.1 + 3.0 * q + float2(8.3, 2.8)));
    float f = fbm(p * 1.7 + 2.4 * r);
    float3 col = jewel(f * 1.1 + (1.0 - uv.y) * 0.18 - 0.08);
    // Thin caustic light where the facets meet, like light moving through a stone.
    float ridge = pow(1.0 - abs(sin(9.0 * f + time * 0.15)), 30.0);
    col += float3(0.45, 0.60, 1.0) * ridge * 0.06;
    float v = smoothstep(1.35, 0.1, length(p));
    col *= 0.62 + 0.38 * v;
    return half4(half3(col), 1.0h);
}

/// Faceted gem shading for a shape: radial facets, a flat table, a slow
/// moving key light, specular glints and a touch of dispersion.
[[stitchable]] half4 gemFacet(float2 pos, half4 color, float2 size, float time, half4 tint) {
    if (color.a <= 0.0h) return color;
    float2 d = (pos - size * 0.5) / (min(size.x, size.y) * 0.5);
    float ang = atan2(d.y, d.x), rad = clamp(length(d), 0.0, 1.0);
    const float facets = 8.0;
    float s = (ang + M_PI_F) / (2.0 * M_PI_F) * facets;
    float seg = floor(s);
    float fa = (seg + 0.5) / facets * 2.0 * M_PI_F - M_PI_F;
    float3 n = normalize(float3(cos(fa) * rad * 0.9, sin(fa) * rad * 0.9, 1.15 - rad * 0.55));
    if (rad < 0.42) n = float3(0, 0, 1);
    float3 L = normalize(float3(-0.45 + 0.35 * sin(time * 0.45), -0.75 + 0.2 * cos(time * 0.3), 0.85));
    float diff = max(dot(n, L), 0.0);
    float spec = pow(max(dot(reflect(-L, n), float3(0, 0, 1)), 0.0), 28.0);
    float3 disp = 0.5 + 0.5 * cos(6.28318 * (float3(0.0, 0.33, 0.67) + seg / facets + time * 0.04));
    float3 base = float3(tint.rgb);
    float3 col = base * (0.55 + 0.55 * diff) + disp * 0.10 * rad + spec * 0.55;
    // Soft facet seams and a bright table read as a cut stone rather than a wheel.
    float edge = abs(fract(s) - 0.5);
    col *= 0.93 + 0.10 * smoothstep(0.0, 0.05, 0.5 - edge);
    col += float3(1.0) * 0.10 * (1.0 - smoothstep(0.30, 0.44, rad));
    col *= 0.85 + 0.15 * (1.0 - smoothstep(0.88, 1.0, rad));
    return half4(half3(col) * color.a, color.a);
}


/// Polished-stone sheen for buttons and tiles: a deep body colour, a soft top
/// highlight, a slow diagonal light sweep and a faint coloured rim. No facet lines.
[[stitchable]] half4 gemSheen(float2 pos, half4 color, float2 size, float time, half4 tint) {
    if (color.a <= 0.0h) return color;
    float2 uv = pos / size;
    float3 base = float3(tint.rgb);
    float3 deep = base * 0.55;
    float3 col = mix(base * 1.08, deep, smoothstep(0.0, 1.0, uv.y));
    // Soft glassy highlight across the top third.
    col += float3(1.0) * 0.16 * (1.0 - smoothstep(0.0, 0.45, uv.y));
    // Slow diagonal light sweep, every ~7 s.
    float phase = fract(time / 7.0) * 2.6 - 0.8;
    float band = exp(-pow((uv.x + uv.y * 0.6 - phase) * 7.0, 2.0));
    col += float3(1.0, 0.97, 1.0) * band * 0.22;
    // Faint dispersion along the lower edge, like light leaving a stone.
    float3 disp = 0.5 + 0.5 * cos(6.28318 * (float3(0.0, 0.33, 0.67) + uv.x * 0.8 + time * 0.03));
    col += disp * 0.10 * smoothstep(0.7, 1.0, uv.y);
    return half4(half3(col) * color.a, color.a);
}
