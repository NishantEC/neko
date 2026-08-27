#!/usr/bin/env python3
"""neko icon v2 — softer form, approachable face."""

# Softer ear tips (rounded, not spiked) and a gentler cheek line.
HEAD = ("M 272 214 C 258 210 246 224 244 252 "
        "C 230 372 224 480 226 574 C 222 700 300 830 512 830 "
        "C 724 830 802 700 798 574 C 800 480 794 372 780 252 "
        "C 778 224 766 210 752 214 "
        "C 706 300 656 356 600 380 C 544 396 480 396 424 380 "
        "C 368 356 318 300 272 214 Z")
# Inner ear: a rounded wedge, inset from the outline so a rim of coat shows.
EAR_L = ("M 300 282 C 318 344 344 384 374 406 C 344 414 318 398 306 372 "
         "C 294 344 294 306 300 282 Z")
EAR_R = ("M 724 282 C 706 344 680 384 650 406 C 680 414 706 398 718 372 "
         "C 730 344 730 306 724 282 Z")
RULE, DY = 820.0, 92.0
EYE_Y = 560 + DY * 0.42


def paw(cx, w=104, h=52):
    return (f'M {cx-w} {RULE} C {cx-w} {RULE-h*0.9} {cx-w*0.66} {RULE-h-8} {cx} {RULE-h-8} '
            f'C {cx+w*0.66} {RULE-h-8} {cx+w} {RULE-h*0.9} {cx+w} {RULE} Z')


def eye(cx, r=66, detail='full'):
    """Round pupil, two highlights. A vertical slit reads as predatory."""
    second = (f'<circle cx="{cx+r*0.34:.0f}" cy="{EYE_Y+r*0.40:.0f}" r="{r*0.13:.0f}" '
              f'fill="#fff" opacity=".5"/>') if detail == "full" else ""
    return f'''
  <ellipse cx="{cx}" cy="{EYE_Y:.0f}" rx="{r}" ry="{r*1.06:.0f}" fill="url(#iris)"/>
  <ellipse cx="{cx}" cy="{EYE_Y+4:.0f}" rx="{r*0.52:.0f}" ry="{r*0.60:.0f}" fill="#0A0D12"/>
  <ellipse cx="{cx-r*0.30:.0f}" cy="{EYE_Y-r*0.42:.0f}" rx="{r*0.27:.0f}" ry="{r*0.24:.0f}"
           fill="#fff" opacity=".95"/>{second}'''


def svg(size=1024, detail="full"):
    tx = f'translate(0 {DY})'
    er = {"full": 66, "mid": 72, "small": 82}[detail]
    eyes = eye(408 if detail == "small" else 408, er, detail) + eye(616, er, detail)
    show_face = detail != "small"
    show_rim = detail != "small"
    rimg = ('<path d="%s" transform="%s translate(-14,-16)" fill="none" stroke="#fff" '
            'stroke-opacity=".20" stroke-width="18" filter="url(#rl)"/>' % (HEAD, tx)) if show_rim else ""
    Y = EYE_Y
    facemarks = ("" if not show_face else
      f'<path d="M 494 {Y+112:.0f} C 500 {Y+106:.0f} 524 {Y+106:.0f} 530 {Y+112:.0f} '
      f'C 530 {Y+126:.0f} 518 {Y+134:.0f} 512 {Y+134:.0f} '
      f'C 506 {Y+134:.0f} 494 {Y+126:.0f} 494 {Y+112:.0f} Z" fill="#E4A0B2" opacity=".95"/>'
      f'<path d="M 512 {Y+134:.0f} L 512 {Y+150:.0f}" stroke="#0F0F16" stroke-width="8" '
      f'stroke-opacity=".8" stroke-linecap="round"/>'
      f'<path d="M 512 {Y+150:.0f} C 496 {Y+172:.0f} 466 {Y+168:.0f} 458 {Y+150:.0f}" fill="none" '
      f'stroke="#0F0F16" stroke-width="9" stroke-opacity=".8" stroke-linecap="round"/>'
      f'<path d="M 512 {Y+150:.0f} C 528 {Y+172:.0f} 558 {Y+168:.0f} 566 {Y+150:.0f}" fill="none" '
      f'stroke="#0F0F16" stroke-width="9" stroke-opacity=".8" stroke-linecap="round"/>')
    return f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024" width="1024" height="1024">
<defs>
 <clipPath id="sq"><rect width="1024" height="1024" rx="229" ry="229"/></clipPath>
 <clipPath id="above"><rect x="0" y="0" width="1024" height="{RULE}"/></clipPath>
 <clipPath id="bc"><path d="{HEAD}" transform="{tx}"/></clipPath>
 <radialGradient id="bg" cx=".42" cy=".3" r=".95">
  <stop offset="0" stop-color="#FBF7ED"/><stop offset=".55" stop-color="#F2EADA"/>
  <stop offset="1" stop-color="#DFD2B8"/></radialGradient>
 <linearGradient id="lg" x1="0" y1="0" x2="0" y2="1">
  <stop offset="0" stop-color="#9C8A6C"/><stop offset=".25" stop-color="#7E6E54"/>
  <stop offset="1" stop-color="#584B39"/></linearGradient>
 <!-- more stops = a smoother falloff; the abrupt one read as clip-art -->
 <radialGradient id="b" cx=".38" cy=".16" r="1.0">
  <stop offset="0" stop-color="#54535F"/><stop offset=".3" stop-color="#35343F"/>
  <stop offset=".62" stop-color="#20202A"/><stop offset=".85" stop-color="#131319"/>
  <stop offset="1" stop-color="#0A0A0F"/></radialGradient>
 <radialGradient id="earg" cx=".3" cy=".2" r="1">
  <stop offset="0" stop-color="#F6C0CE"/><stop offset="1" stop-color="#D07E96"/></radialGradient>
 <radialGradient id="iris" cx=".38" cy=".3" r=".85">
  <stop offset="0" stop-color="#A8F0C4"/><stop offset=".5" stop-color="#5CC98A"/>
  <stop offset="1" stop-color="#2E8A5A"/></radialGradient>
 <radialGradient id="muz" cx=".5" cy=".2" r=".9">
  <stop offset="0" stop-color="#fff" stop-opacity=".14"/>
  <stop offset="1" stop-color="#fff" stop-opacity="0"/></radialGradient>
 <linearGradient id="gl" x1=".2" y1="0" x2=".6" y2="1">
  <stop offset="0" stop-color="#fff" stop-opacity=".26"/>
  <stop offset="1" stop-color="#fff" stop-opacity="0"/></linearGradient>
 <filter id="sf" x="-40%" y="-40%" width="180%" height="180%"><feGaussianBlur stdDeviation="30"/></filter>
 <filter id="tt" x="-40%" y="-40%" width="180%" height="180%"><feGaussianBlur stdDeviation="16"/></filter>
 <filter id="rl" x="-40%" y="-40%" width="180%" height="180%"><feGaussianBlur stdDeviation="13"/></filter>
 <filter id="soft6" x="-40%" y="-40%" width="180%" height="180%"><feGaussianBlur stdDeviation="6"/></filter>
</defs>
<g clip-path="url(#sq)">
 <rect width="1024" height="1024" fill="url(#bg)"/>
 <g clip-path="url(#above)">
  <path d="{HEAD}" transform="{tx}" fill="#4A4030" opacity=".26" filter="url(#sf)"/>
  <path d="{HEAD}" transform="{tx}" fill="url(#b)"/>
  <g clip-path="url(#bc)">
   <ellipse cx="428" cy="{292+DY:.0f}" rx="224" ry="132" fill="url(#gl)"/>
   <!-- muzzle: a lift in the coat, not a drawn shape -->
   <ellipse cx="512" cy="{EYE_Y+150:.0f}" rx="196" ry="128" fill="url(#muz)"/>
   {rimg}
  </g>
  <path d="{EAR_L}" transform="{tx}" fill="url(#earg)" opacity=".93" filter="url(#soft6)"/>
  <path d="{EAR_R}" transform="{tx}" fill="url(#earg)" opacity=".93" filter="url(#soft6)"/>
  {eyes}
  {facemarks}
  <path d="M 512 {EYE_Y+150:.0f} C 496 {EYE_Y+172:.0f} 466 {EYE_Y+168:.0f} 458 {EYE_Y+150:.0f}"
        fill="none" stroke="#0F0F16" stroke-width="9" stroke-opacity=".8" stroke-linecap="round"/>
  <path d="M 512 {EYE_Y+150:.0f} C 528 {EYE_Y+172:.0f} 558 {EYE_Y+168:.0f} 566 {EYE_Y+150:.0f}"
        fill="none" stroke="#0F0F16" stroke-width="9" stroke-opacity=".8" stroke-linecap="round"/>
 </g>
 <rect x="0" y="{RULE}" width="1024" height="{1024-RULE}" fill="url(#lg)"/>
 <rect x="0" y="{RULE}" width="1024" height="3" fill="#fff" opacity=".10"/>
 <g><path d="{paw(330)}" fill="url(#b)"/><path d="{paw(694)}" fill="url(#b)"/><path d="{paw(330)}" fill="none" stroke="#fff" stroke-opacity=".16" stroke-width="6" filter="url(#soft6)"/><path d="{paw(694)}" fill="none" stroke="#fff" stroke-opacity=".16" stroke-width="6" filter="url(#soft6)"/></g>
 <ellipse cx="512" cy="{RULE+4}" rx="262" ry="18" fill="#3A3020" opacity=".38" filter="url(#tt)"/>
</g></svg>'''


PLAN = [(1024,"full"),(512,"full"),(256,"full"),(128,"mid"),(64,"mid"),(32,"small"),(16,"small")]

if __name__ == "__main__":
    for sz, d in PLAN:
        open(f"/tmp/neko-v2/n{sz}.svg", "w").write(svg(sz, d))
    print("wrote", len(PLAN))
