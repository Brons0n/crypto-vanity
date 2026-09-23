// Original portable integer kernel. All key material stays on the CPU.
// Device state contains only public Jacobian points and public matching criteria.
#ifdef __METAL_VERSION__
#include <metal_stdlib>
using namespace metal;
#define PRIVATE thread
#define CONSTANT constant
#define GLOBAL device
#else
#define PRIVATE __private
#define CONSTANT __constant
#define GLOBAL __global
#endif
#define INLINE __attribute__((always_inline)) static inline
#define STEPS 16

typedef struct {
    uint v[8];
} Fe;
typedef struct {
    Fe x;
    Fe y;
    Fe z;
} Point;
CONSTANT uint PRIME[8] = {0xfffffc2f, 0xfffffffe, 0xffffffff, 0xffffffff,
                          0xffffffff, 0xffffffff, 0xffffffff, 0xffffffff};
INLINE Fe small(uint n) {
    Fe a = {{0}};
    a.v[0] = n;
    return a;
}
INLINE int zero(Fe a) {
    uint t = 0;
    for (int i = 0; i < 8; i++)
        t |= a.v[i];
    return t == 0;
}
INLINE int ge_p(Fe a) {
    for (int i = 7; i >= 0; i--) {
        if (a.v[i] != PRIME[i])
            return a.v[i] > PRIME[i];
    }
    return 1;
}
INLINE Fe sub_p(Fe a) {
    ulong borrow = 0;
    for (int i = 0; i < 8; i++) {
        ulong b = (ulong)PRIME[i] + borrow;
        uint old = a.v[i];
        a.v[i] = (uint)((ulong)old - b);
        borrow = ((ulong)old < b);
    }
    return a;
}
INLINE Fe add(Fe a, Fe b) {
    Fe r;
    ulong carry = 0;
    for (int i = 0; i < 8; i++) {
        ulong t = (ulong)a.v[i] + b.v[i] + carry;
        r.v[i] = (uint)t;
        carry = t >> 32;
    }
    if (carry || ge_p(r))
        r = sub_p(r);
    return r;
}
INLINE Fe sub(Fe a, Fe b) {
    Fe r;
    ulong borrow = 0;
    for (int i = 0; i < 8; i++) {
        ulong t = (ulong)b.v[i] + borrow;
        r.v[i] = (uint)((ulong)a.v[i] - t);
        borrow = ((ulong)a.v[i] < t);
    }
    if (borrow) {
        ulong carry = 0;
        for (int i = 0; i < 8; i++) {
            ulong t = (ulong)r.v[i] + PRIME[i] + carry;
            r.v[i] = (uint)t;
            carry = t >> 32;
        }
    }
    return r;
}
INLINE Fe mul(Fe a, Fe b) {
    uint t[16] = {0};
    for (int i = 0; i < 8; i++) {
        ulong carry = 0;
        for (int j = 0; j < 8; j++) {
            ulong x = (ulong)a.v[i] * b.v[j] + t[i + j] + carry;
            t[i + j] = (uint)x;
            carry = x >> 32;
        }
        t[i + 8] = (uint)carry;
    }
    // 2^256 = 2^32 + 977 (mod secp256k1's field prime).
    ulong r[10] = {0};
    for (int i = 0; i < 8; i++) {
        r[i] += (ulong)t[i] + (ulong)t[i + 8] * 977;
        r[i + 1] += t[i + 8];
    }
    for (int i = 0; i < 9; i++) {
        r[i + 1] += r[i] >> 32;
        r[i] &= 0xffffffffUL;
    }
    while (r[8] || r[9]) {
        ulong h = r[8], h2 = r[9];
        r[8] = 0;
        r[9] = 0;
        r[0] += h * 977;
        r[1] += h + h2 * 977;
        r[2] += h2;
        for (int i = 0; i < 9; i++) {
            r[i + 1] += r[i] >> 32;
            r[i] &= 0xffffffffUL;
        }
    }
    Fe out;
    for (int i = 0; i < 8; i++)
        out.v[i] = (uint)r[i];
    if (ge_p(out))
        out = sub_p(out);
    return out;
}
INLINE Fe sq(Fe a) { return mul(a, a); }
INLINE Fe inv(Fe a) {
    Fe r = small(1);
    for (int i = 255; i >= 0; i--) {
        r = sq(r);
        uint word = PRIME[i / 32];
        if (i / 32 == 0)
            word -= 2;
        if ((word >> (i % 32)) & 1)
            r = mul(r, a);
    }
    return r;
}
INLINE Point twice(Point p) {
    Fe a = sq(p.x), b = sq(p.y), c = sq(b);
    Fe d = sub(sub(sq(add(p.x, b)), a), c);
    d = add(d, d);
    Fe e = add(add(a, a), a), f = sq(e);
    Point r;
    r.x = sub(f, add(d, d));
    Fe eight = add(c, c);
    eight = add(eight, eight);
    eight = add(eight, eight);
    r.y = sub(mul(e, sub(d, r.x)), eight);
    r.z = mul(add(p.y, p.y), p.z);
    return r;
}
INLINE Point next_point(Point p) {
    Fe gx = {{0x16f81798, 0x59f2815b, 0x2dce28d9, 0x029bfcdb, 0xce870b07, 0x55a06295, 0xf9dcbbac,
              0x79be667e}};
    Fe gy = {{0xfb10d4b8, 0x9c47d08f, 0xa6855419, 0xfd17b448, 0x0e1108a8, 0x5da4fbfc, 0x26a3c465,
              0x483ada77}};
    if (zero(p.z)) {
        Point g = {gx, gy, small(1)};
        return g;
    }
    Fe zz = sq(p.z), u = mul(gx, zz), s = mul(gy, mul(p.z, zz));
    Fe h = sub(u, p.x), r = sub(s, p.y);
    if (zero(h)) {
        if (zero(r))
            return twice(p);
        Point inf = {small(0), small(1), small(0)};
        return inf;
    }
    Fe hh = sq(h), hhh = mul(h, hh), v = mul(p.x, hh);
    Point out;
    out.x = sub(sub(sq(r), hhh), add(v, v));
    out.y = sub(mul(r, sub(v, out.x)), mul(p.y, hhh));
    out.z = mul(p.z, h);
    return out;
}
INLINE uint rr(uint x, uint n) { return (x >> n) | (x << (32 - n)); }
INLINE uint rl(uint x, uint n) { return (x << n) | (x >> (32 - n)); }
CONSTANT uint SHA_K[64] = {
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2};
// All inputs in this application fit one SHA-256 block.
INLINE void sha(PRIVATE const uchar *input, uint len, PRIVATE uchar *output) {
    uint w[64] = {0};
    for (uint i = 0; i < len; i++)
        w[i / 4] |= (uint)input[i] << (24 - 8 * (i % 4));
    w[len / 4] |= 0x80u << (24 - 8 * (len % 4));
    w[15] = len * 8;
    for (int i = 16; i < 64; i++) {
        uint x = w[i - 15], y = w[i - 2];
        w[i] = w[i - 16] + (rr(x, 7) ^ rr(x, 18) ^ (x >> 3)) + w[i - 7] +
               (rr(y, 17) ^ rr(y, 19) ^ (y >> 10));
    }
    uint a = 0x6a09e667, b = 0xbb67ae85, c = 0x3c6ef372, d = 0xa54ff53a, e = 0x510e527f,
         f = 0x9b05688c, g = 0x1f83d9ab, h = 0x5be0cd19;
    for (int i = 0; i < 64; i++) {
        uint t = h + (rr(e, 6) ^ rr(e, 11) ^ rr(e, 25)) + ((e & f) ^ ((~e) & g)) + SHA_K[i] + w[i];
        uint u = (rr(a, 2) ^ rr(a, 13) ^ rr(a, 22)) + ((a & b) ^ (a & c) ^ (b & c));
        h = g;
        g = f;
        f = e;
        e = d + t;
        d = c;
        c = b;
        b = a;
        a = t + u;
    }
    uint state[8] = {a + 0x6a09e667, b + 0xbb67ae85, c + 0x3c6ef372, d + 0xa54ff53a,
                     e + 0x510e527f, f + 0x9b05688c, g + 0x1f83d9ab, h + 0x5be0cd19};
    for (int i = 0; i < 32; i++)
        output[i] = (uchar)(state[i / 4] >> (24 - 8 * (i % 4)));
}
CONSTANT uchar RL_IDX[80] = {0, 1,  2,  3,  4,  5,  6,  7,  8,  9, 10, 11, 12, 13, 14, 15,
                             7, 4,  13, 1,  10, 6,  15, 3,  12, 0, 9,  5,  2,  14, 11, 8,
                             3, 10, 14, 4,  9,  15, 8,  1,  2,  7, 0,  6,  13, 11, 5,  12,
                             1, 9,  11, 10, 0,  8,  12, 4,  13, 3, 7,  15, 14, 5,  6,  2,
                             4, 0,  5,  9,  7,  12, 2,  10, 14, 1, 3,  8,  11, 6,  15, 13};
CONSTANT uchar RR_IDX[80] = {5,  14, 7,  0, 9, 2,  11, 4,  13, 6,  15, 8,  1,  10, 3,  12,
                             6,  11, 3,  7, 0, 13, 5,  10, 14, 15, 8,  12, 4,  9,  1,  2,
                             15, 5,  1,  3, 7, 14, 6,  9,  11, 8,  12, 2,  10, 0,  4,  13,
                             8,  6,  4,  1, 3, 11, 15, 0,  5,  12, 2,  13, 9,  7,  10, 14,
                             12, 15, 10, 4, 1, 5,  8,  7,  6,  2,  13, 14, 0,  3,  9,  11};
CONSTANT uchar RL_S[80] = {11, 14, 15, 12, 5,  8,  7,  9,  11, 13, 14, 15, 6,  7,  9,  8,
                           7,  6,  8,  13, 11, 9,  7,  15, 7,  12, 15, 9,  11, 7,  13, 12,
                           11, 13, 6,  7,  14, 9,  13, 15, 14, 8,  13, 6,  5,  12, 7,  5,
                           11, 12, 14, 15, 14, 15, 9,  8,  9,  14, 5,  6,  8,  6,  5,  12,
                           9,  15, 5,  11, 6,  8,  13, 12, 5,  12, 13, 14, 11, 8,  5,  6};
CONSTANT uchar RR_S[80] = {8,  9,  9,  11, 13, 15, 15, 5,  7,  7,  8,  11, 14, 14, 12, 6,
                           9,  13, 15, 7,  12, 8,  9,  11, 7,  7,  12, 7,  6,  15, 13, 11,
                           9,  7,  15, 11, 8,  6,  6,  14, 12, 13, 5,  14, 13, 13, 7,  5,
                           15, 5,  8,  11, 14, 14, 6,  14, 6,  9,  12, 9,  12, 5,  15, 8,
                           8,  5,  12, 9,  12, 5,  14, 6,  8,  13, 6,  5,  15, 13, 11, 11};
CONSTANT uint KL[5] = {0, 0x5a827999, 0x6ed9eba1, 0x8f1bbcdc, 0xa953fd4e};
CONSTANT uint KR[5] = {0x50a28be6, 0x5c4dd124, 0x6d703ef3, 0x7a6d76e9, 0};
INLINE uint rf(int j, uint x, uint y, uint z) {
    if (j == 0)
        return x ^ y ^ z;
    if (j == 1)
        return (x & y) | (~x & z);
    if (j == 2)
        return (x | ~y) ^ z;
    if (j == 3)
        return (x & z) | (y & ~z);
    return x ^ (y | ~z);
}
INLINE void ripemd(PRIVATE const uchar *input, PRIVATE uchar *output) {
    uint w[16] = {0};
    for (int i = 0; i < 32; i++)
        w[i / 4] |= (uint)input[i] << (8 * (i % 4));
    w[8] = 0x80;
    w[14] = 256;
    uint a = 0x67452301, b = 0xefcdab89, c = 0x98badcfe, d = 0x10325476, e = 0xc3d2e1f0;
    uint aa = a, bb = b, cc = c, dd = d, ee = e;
    for (int i = 0; i < 80; i++) {
        int j = i / 16;
        uint t = rl(a + rf(j, b, c, d) + w[RL_IDX[i]] + KL[j], RL_S[i]) + e;
        a = e;
        e = d;
        d = rl(c, 10);
        c = b;
        b = t;
        t = rl(aa + rf(4 - j, bb, cc, dd) + w[RR_IDX[i]] + KR[j], RR_S[i]) + ee;
        aa = ee;
        ee = dd;
        dd = rl(cc, 10);
        cc = bb;
        bb = t;
    }
    uint state[5] = {0xefcdab89 + c + dd, 0x98badcfe + d + ee, 0x10325476 + e + aa,
                     0xc3d2e1f0 + a + bb, 0x67452301 + b + cc};
    for (int i = 0; i < 20; i++)
        output[i] = (uchar)(state[i / 4] >> (8 * (i % 4)));
}
CONSTANT char B58[] = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
CONSTANT char B32[] = "qpzry9x8gf2tvdw0s3jn54khce6mua7l";
CONSTANT uint BCH[5] = {0x3b6a57b2, 0x26508e6d, 0x1ea119fa, 0x3d4233dd, 0x2a1462b3};
INLINE uint polymod(uint c, uint value) {
    uint top = c >> 25;
    c = ((c & 0x1ffffff) << 5) ^ value;
    for (int i = 0; i < 5; i++)
        if ((top >> i) & 1)
            c ^= BCH[i];
    return c;
}
INLINE uint encode(Fe x, Fe y, uint kind, PRIVATE uchar *address) {
    uchar pub[33], digest[32], hash[20];
    pub[0] = 2 + (y.v[0] & 1);
    for (int i = 0; i < 32; i++)
        pub[i + 1] = (uchar)(x.v[7 - i / 4] >> (24 - 8 * (i % 4)));
    sha(pub, 33, digest);
    ripemd(digest, hash);
    if (kind) {
        address[0] = 'b';
        address[1] = 'c';
        address[2] = '1';
        address[3] = 'q';
        uint chk = 1;
        uint hrp[6] = {3, 3, 0, 2, 3, 0};
        for (int i = 0; i < 6; i++)
            chk = polymod(chk, hrp[i]);
        uint acc = 0, bits = 0, n = 4;
        for (int i = 0; i < 20; i++) {
            acc = (acc << 8) | hash[i];
            bits += 8;
            while (bits >= 5) {
                bits -= 5;
                uint v = (acc >> bits) & 31;
                address[n++] = B32[v];
                chk = polymod(chk, v);
            }
        }
        for (int i = 0; i < 6; i++)
            chk = polymod(chk, 0);
        chk ^= 1;
        for (int i = 0; i < 6; i++)
            address[n++] = B32[(chk >> (5 * (5 - i))) & 31];
        return n;
    }
    uchar data[25] = {0}, second[32];
    for (int i = 0; i < 20; i++)
        data[i + 1] = hash[i];
    sha(data, 21, digest);
    sha(digest, 32, second);
    for (int i = 0; i < 4; i++)
        data[21 + i] = second[i];
    uint leading = 0;
    while (leading < 25 && !data[leading])
        leading++;
    uchar digits[35] = {0};
    uint used = 0;
    for (int i = 0; i < 25; i++) {
        uint carry = data[i];
        for (uint j = 0; j < used; j++) {
            carry += 256u * digits[j];
            digits[j] = carry % 58;
            carry /= 58;
        }
        while (carry) {
            digits[used++] = carry % 58;
            carry /= 58;
        }
    }
    uint n = 0;
    for (uint i = 0; i < leading; i++)
        address[n++] = '1';
    for (int i = (int)used - 1; i >= 0; i--)
        address[n++] = B58[digits[i]];
    return n;
}
INLINE uchar lower(uchar c) { return c >= 'A' && c <= 'Z' ? c + 32 : c; }
// params: kind, case-insensitive, prefix length, suffix length, then bytes as u32.
INLINE int matches(PRIVATE uchar *addr, uint len, GLOBAL const uint *params) {
    uint fixed = params[0] ? 4 : 1, pre = params[2], suf = params[3];
    if (fixed + pre > len || suf > len)
        return 0;
    for (uint i = 0; i < pre + suf; i++) {
        uchar a = i < pre ? addr[fixed + i] : addr[len - suf + i - pre], b = (uchar)params[4 + i];
        if (params[1]) {
            a = lower(a);
            b = lower(b);
        }
        if (a != b)
            return 0;
    }
    return 1;
}
INLINE void search_lane(uint lane, GLOBAL uint *points, GLOBAL const uint *params,
                        GLOBAL uint *hits) {
    Point p;
    for (int i = 0; i < 8; i++) {
        p.x.v[i] = points[lane * 24 + i];
        p.y.v[i] = points[lane * 24 + 8 + i];
        p.z.v[i] = points[lane * 24 + 16 + i];
    }
    Point batch[STEPS];
    Fe products[STEPS];
    Fe product = small(1);
    // Montgomery batch inversion: one inversion for 16 sequential public points.
    for (int i = 0; i < STEPS; i++) {
        batch[i] = p;
        products[i] = product;
        product = mul(product, p.z);
        p = next_point(p);
    }
    Fe inverse = inv(product);
    uint hit = 0;
    for (int i = STEPS - 1; i >= 0; i--) {
        Fe zi = mul(inverse, products[i]);
        inverse = mul(inverse, batch[i].z);
        Fe zi2 = sq(zi);
        Fe x = mul(batch[i].x, zi2), y = mul(batch[i].y, mul(zi, zi2));
        uchar addr[42];
        uint n = encode(x, y, params[0], addr);
        if (matches(addr, n, params))
            hit = (uint)i + 1;
    }
    // Private scalars never cross the GPU boundary. Only a lane/offset is returned.
    hits[lane] = hit;
    if (hit) {
#ifdef __METAL_VERSION__
        atomic_fetch_or_explicit((device atomic_uint *)&hits[256], 1u, memory_order_relaxed);
#else
        atomic_or((volatile GLOBAL uint *)&hits[256], 1u);
#endif
    }
    for (int i = 0; i < 8; i++) {
        points[lane * 24 + i] = p.x.v[i];
        points[lane * 24 + 8 + i] = p.y.v[i];
        points[lane * 24 + 16 + i] = p.z.v[i];
    }
}
