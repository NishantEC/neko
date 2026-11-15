#!/usr/bin/env python3
"""Writes assets/sample.pdf: two pages with standard-14 text, vector paths and a raster image.
Hand-made, so it carries no third-party licence. Usage: make_sample_pdf.py assets/sample.pdf"""
import sys, zlib

W, H = 612, 792
img_w = img_h = 64
img = bytearray()
for y in range(img_h):
    for x in range(img_w):
        img += bytes([int(255 * x / img_w), int(255 * y / img_h), 160])
img_z = zlib.compress(bytes(img))

page1 = b"""
BT /F1 28 Tf 56 720 Td (Neko canvas: PDF through hayro) Tj ET
BT /F2 12 Tf 56 690 Td (Rendered on the CPU by a pure Rust crate, shown with El::pixels.) Tj ET
0.86 0.24 0.2 rg 56 560 200 90 re f
0.1 0.45 0.85 RG 3 w 280 560 200 90 re S
0.1 0.6 0.3 rg 380 470 m 440 540 500 470 560 540 c 560 460 l 380 460 l f
q 128 0 0 128 56 380 cm /Im1 Do Q
BT /F3 11 Tf 56 340 Td (Courier text: fn main() { println!("hello"); }) Tj ET
BT /F1 11 Tf 56 320 Td (The quick brown fox jumps over the lazy dog 0123456789) Tj ET
0.3 w 56 300 m 556 300 l S
"""
page2 = b"""
BT /F1 22 Tf 56 720 Td (Page two) Tj ET
""" + b"".join(
    b"BT /F2 12 Tf 56 %d Td (Row %d      value %d      status %s) Tj ET\n" % (680 - i * 18, i + 1, (i * 37) % 101, b"ok" if i % 3 else b"check")
    for i in range(30)
)

objs = []
def add(body):
    objs.append(body)
    return len(objs)

font = lambda name: add(b"<< /Type /Font /Subtype /Type1 /BaseFont /%s >>" % name)
f1, f2, f3 = font(b"Helvetica-Bold"), font(b"Times-Roman"), font(b"Courier")
im = add(b"<< /Type /XObject /Subtype /Image /Width %d /Height %d /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode /Length %d >>\nstream\n" % (img_w, img_h, len(img_z)) + img_z + b"\nendstream")
def stream(data):
    return add(b"<< /Length %d >>\nstream\n" % len(data) + data + b"\nendstream")
c1, c2 = stream(page1), stream(page2)
pages_id = len(objs) + 3  # page1, page2, then Pages
res = b"<< /Font << /F1 %d 0 R /F2 %d 0 R /F3 %d 0 R >> /XObject << /Im1 %d 0 R >> >>" % (f1, f2, f3, im)
p1 = add(b"<< /Type /Page /Parent %d 0 R /MediaBox [0 0 %d %d] /Contents %d 0 R /Resources %s >>" % (pages_id, W, H, c1, res))
p2 = add(b"<< /Type /Page /Parent %d 0 R /MediaBox [0 0 %d %d] /Contents %d 0 R /Resources %s >>" % (pages_id, W, H, c2, res))
pg = add(b"<< /Type /Pages /Kids [%d 0 R %d 0 R] /Count 2 >>" % (p1, p2))
assert pg == pages_id
cat = add(b"<< /Type /Catalog /Pages %d 0 R >>" % pg)

out = bytearray(b"%PDF-1.4\n")
offs = []
for i, body in enumerate(objs, 1):
    offs.append(len(out))
    out += b"%d 0 obj\n" % i + body + b"\nendobj\n"
xref = len(out)
out += b"xref\n0 %d\n0000000000 65535 f \n" % (len(objs) + 1)
for o in offs:
    out += b"%010d 00000 n \n" % o
out += b"trailer\n<< /Size %d /Root %d 0 R >>\nstartxref\n%d\n%%%%EOF\n" % (len(objs) + 1, cat, xref)
open(sys.argv[1], "wb").write(out)
print("wrote", sys.argv[1], len(out), "bytes")
