"""Finds Phosphor icon codepoints by icon name, through the font's ligature table.

Usage: python scripts/phosphor_icon.py studio/assets/fonts/Phosphor.ttf cursor user-plus
Prints `LIG <name> U+XXXX` per name (or `missing`), for the consts in studio/src/icons.rs.
"""
import struct, sys

data = open(sys.argv[1], 'rb').read()
num = struct.unpack('>H', data[4:6])[0]
tables = {}
for i in range(num):
    tag, _, off, ln = struct.unpack('>4sIII', data[12 + 16 * i: 28 + 16 * i])
    tables[tag.decode()] = (off, ln)

off, _ = tables['post']
names = []
if struct.unpack('>I', data[off:off + 4])[0] == 0x00020000:
    n = struct.unpack('>H', data[off + 32:off + 34])[0]
    idx = struct.unpack('>%dH' % n, data[off + 34:off + 34 + 2 * n])
    p = off + 34 + 2 * n
    extra = []
    end = off + tables['post'][1]
    while p < end:
        l = data[p]
        extra.append(data[p + 1:p + 1 + l].decode('latin1'))
        p += 1 + l
    for i in idx:
        names.append(extra[i - 258] if i >= 258 else '#%d' % i)

off, _ = tables['cmap']
n = struct.unpack('>H', data[off + 2:off + 4])[0]
cmap = {}
for i in range(n):
    pid, eid, so = struct.unpack('>HHI', data[off + 4 + 8 * i: off + 12 + 8 * i])
    st = off + so
    fmt = struct.unpack('>H', data[st:st + 2])[0]
    if fmt == 12:
        groups = struct.unpack('>I', data[st + 12:st + 16])[0]
        for g in range(groups):
            a, b, gid = struct.unpack('>III', data[st + 16 + 12 * g: st + 28 + 12 * g])
            for c in range(a, b + 1):
                cmap[c] = gid + c - a
    elif fmt == 4:
        segx2 = struct.unpack('>H', data[st + 6:st + 8])[0]
        seg = segx2 // 2
        ends = struct.unpack('>%dH' % seg, data[st + 14:st + 14 + segx2])
        starts = struct.unpack('>%dH' % seg, data[st + 16 + segx2:st + 16 + 2 * segx2])
        deltas = struct.unpack('>%dh' % seg, data[st + 16 + 2 * segx2:st + 16 + 3 * segx2])
        ro_off = st + 16 + 3 * segx2
        ros = struct.unpack('>%dH' % seg, data[ro_off:ro_off + segx2])
        for s in range(seg):
            for c in range(starts[s], ends[s] + 1):
                if c == 0xFFFF:
                    continue
                if ros[s] == 0:
                    gid = (c + deltas[s]) & 0xFFFF
                else:
                    a = ro_off + 2 * s + ros[s] + 2 * (c - starts[s])
                    gid = struct.unpack('>H', data[a:a + 2])[0]
                    if gid:
                        gid = (gid + deltas[s]) & 0xFFFF
                cmap.setdefault(c, gid)

want = sys.argv[2:]
for c, g in sorted(cmap.items()):
    name = names[g] if g < len(names) else '?'
    if not want or any(w == name for w in want):
        print('%s U+%04X' % (name, c))

# Ligature names (GSUB lookup type 4): spelled glyph sequence -> glyph id.
def u16(o): return struct.unpack('>H', data[o:o + 2])[0]
gid_to_char = {}
for c, g in cmap.items():
    if c < 0x80:
        gid_to_char[g] = chr(c)
gid_to_cp = {}
for c, g in cmap.items():
    if c >= 0xE000:
        gid_to_cp.setdefault(g, c)
if 'GSUB' in tables:
    gs = tables['GSUB'][0]
    lookup_list = gs + u16(gs + 8)
    lig_names = {}
    def coverage(o):
        fmt = u16(o)
        out = []
        if fmt == 1:
            n = u16(o + 2)
            out = [u16(o + 4 + 2 * i) for i in range(n)]
        else:
            n = u16(o + 2)
            for i in range(n):
                a, b = u16(o + 4 + 6 * i), u16(o + 6 + 6 * i)
                out.extend(range(a, b + 1))
        return out
    for li in range(u16(lookup_list)):
        lk = lookup_list + u16(lookup_list + 2 + 2 * li)
        ltype = u16(lk)
        for si in range(u16(lk + 4)):
            st = lk + u16(lk + 6 + 2 * si)
            t = ltype
            if t == 7:
                t = u16(st + 2)
                st = st + struct.unpack('>I', data[st + 4:st + 8])[0]
            if t != 4:
                continue
            cov = coverage(st + u16(st + 2))
            for k, first in enumerate(cov):
                ls = st + u16(st + 6 + 2 * k)
                for j in range(u16(ls)):
                    lg = ls + u16(ls + 2 + 2 * j)
                    lig = u16(lg)
                    cnt = u16(lg + 2)
                    comps = [first] + [u16(lg + 4 + 2 * x) for x in range(cnt - 1)]
                    name = ''.join(gid_to_char.get(g, '?') for g in comps)
                    lig_names[name] = lig
    for w in want:
        g = lig_names.get(w)
        cp = gid_to_cp.get(g) if g is not None else None
        print('LIG %s %s' % (w, ('U+%04X' % cp) if cp else 'missing'))
