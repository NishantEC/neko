#!/usr/bin/env python3
"""neko.icns — black cat peeking over a ledge, cream ground.

Plain face: eyes only. No nose, no mouth. Seven drawings, not one scaled.
"""

HEAD = ("M 264 190 C 232 360 224 480 226 574 C 222 700 300 830 512 830 "
        "C 724 830 802 700 798 574 C 800 480 792 360 760 190 "
        "C 710 288 658 350 600 376 C 544 392 480 392 424 376 "
        "C 366 350 314 288 264 190 Z")
EAR_L = "M 292 268 C 314 336 342 378 376 400 C 336 406 308 384 298 352 C 288 322 288 292 292 268 Z"
EAR_R = "M 732 268 C 710 336 682 378 648 400 C 688 406 716 384 726 352 C 736 322 736 292 732 268 Z"

BG1, BG2 = "#F7F1E4", "#E2D5BC"          # cream ground
LG1, LG2 = "#8A7A5F", "#5E5140"          # ledge
EYE = "#4FBF7A"
RULE, DY = 820.0, 92.0
EYE_Y = 560 + DY * 0.42                  # 598.6


def paw(cx, w=96, h=54):
    return (f'M {cx-w} {RULE} C {cx-w} {RULE-h} {cx-w*0.62} {RULE-h-10} {cx} {RULE-h-10} '
            f'C {cx+w*0.62} {RULE-h-10} {cx+w} {RULE-h} {cx+w} {RULE} Z')


def svg(detail):
    """detail: 'full' (>=256px) | 'mid' (64-128) | 'small' (<=32)."""
    tx = f'translate(0 {DY})'
    y = EYE_Y
    # eyes grow as everything else is stripped away — at 16px they are the icon
    rx, ry = {"full": (58, 70), "mid": (63, 75), "small": (72, 84)}[detail]
    rim = {"full": 0.22, "mid": 0.30, "small": 0.0}[detail]
    slit = detail != "small"
    shine = detail == "full"
    gloss = detail != "small"
    ears = (f'<path d="{EAR_L}" transform="{tx}" fill="#E88BA8" opacity=".85"/>'
            f'<path d="{EAR_R}" transform="{tx}" fill="#E88BA8" opacity=".85"/>'
            if detail != "small" else "")
    glossg = (f'<ellipse cx="430" cy="{300+DY:.0f}" rx="215" ry="126" fill="url(#gl)"/>'
              if gloss else "")
    rimg = (f'<path d="{HEAD}" transform="{tx} translate(-13,-15)" fill="none" stroke="#fff" '
            f'stroke-opacity="{rim}" stroke-width="17" filter="url(#rl)"/>' if rim else "")
    eyes = "".join(
        f'<ellipse cx="{x}" cy="{y:.0f}" rx="{rx}" ry="{ry}" fill="{EYE}"/>'
        + (f'<ellipse cx="{x}" cy="{y:.0f}" rx="17" ry="{ry-14}" fill="#0B0B10"/>' if slit else "")
        + (f'<circle cx="{x-22}" cy="{y-30:.0f}" r="15" fill="#fff" opacity=".95"/>' if shine else "")
        for x in (404, 620))
    return f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024" width="1024" height="1024">
<defs>
 <clipPath id="sq"><rect width="1024" height="1024" rx="229" ry="229"/></clipPath>
 <clipPath id="above"><rect x="0" y="0" width="1024" height="{RULE}"/></clipPath>
 <clipPath id="bc"><path d="{HEAD}" transform="{tx}"/></clipPath>
 <linearGradient id="bg" x1="0" y1="0" x2=".35" y2="1">
  <stop offset="0" stop-color="{BG1}"/><stop offset="1" stop-color="{BG2}"/></linearGradient>
 <linearGradient id="lg" x1="0" y1="0" x2="0" y2="1">
  <stop offset="0" stop-color="{LG1}"/><stop offset="1" stop-color="{LG2}"/></linearGradient>
 <radialGradient id="b" cx=".36" cy=".18" r=".95">
  <stop offset="0" stop-color="#43434F"/><stop offset=".55" stop-color="#1E1E26"/>
  <stop offset="1" stop-color="#08080C"/></radialGradient>
 <linearGradient id="gl" x1="0" y1="0" x2="0" y2="1">
  <stop offset="0" stop-color="#fff" stop-opacity=".22"/>
  <stop offset="1" stop-color="#fff" stop-opacity="0"/></linearGradient>
 <filter id="sf" x="-40%" y="-40%" width="180%" height="180%"><feGaussianBlur stdDeviation="28"/></filter>
 <filter id="tt" x="-40%" y="-40%" width="180%" height="180%"><feGaussianBlur stdDeviation="14"/></filter>
 <filter id="rl" x="-40%" y="-40%" width="180%" height="180%"><feGaussianBlur stdDeviation="10"/></filter>
</defs>
<g clip-path="url(#sq)">
 <rect width="1024" height="1024" fill="url(#bg)"/>
 <g clip-path="url(#above)">
  <path d="{HEAD}" transform="{tx}" fill="#000" opacity=".34" filter="url(#sf)"/>
  <path d="{HEAD}" transform="{tx}" fill="url(#b)"/>
  <g clip-path="url(#bc)">{glossg}{rimg}</g>
  {ears}{eyes}
 </g>
 <rect x="0" y="{RULE}" width="1024" height="{1024-RULE}" fill="url(#lg)"/>
 <rect x="0" y="{RULE}" width="1024" height="4" fill="#fff" opacity=".14"/>
 <path d="{paw(408)}" fill="url(#b)"/><path d="{paw(616)}" fill="url(#b)"/>
 <ellipse cx="512" cy="{RULE}" rx="250" ry="20" fill="#000" opacity=".4" filter="url(#tt)"/>
</g></svg>'''


PLAN = [(1024, "full"), (512, "full"), (256, "full"),
        (128, "mid"), (64, "mid"), (32, "small"), (16, "small")]

if __name__ == "__main__":
    for size, detail in PLAN:
        open(f"n{size}.svg", "w").write(svg(detail))
    print("wrote", len(PLAN), "sizes — plain face, no nose or mouth")
