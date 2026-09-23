#!/usr/bin/env python3
"""Offline test of the actual shared GPU arithmetic against Python big integers.

Requires Python 3 and clang on macOS/Linux. This is a developer test only;
production key generation remains entirely in Rust and OS CSPRNGs.
No GPU is needed. Hardware tests additionally check the device compiler.
"""
import ctypes
import hashlib
import pathlib
import subprocess
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[1]
P = 2**256 - 2**32 - 977
G = (
    0x79BE667EF9DCBBAC55A06295CE870B07029BFCDB2DCE28D959F2815B16F81798,
    0x483ADA7726A3C4655DA4FBFC0E1108A8FD17B448A68554199C47D08FFB10D4B8,
)

class Fe(ctypes.Structure):
    _fields_ = [("v", ctypes.c_uint32 * 8)]

    @classmethod
    def of(cls, x):
        return cls((ctypes.c_uint32 * 8)(*((x >> (32*i)) & 0xffffffff for i in range(8))))

    def value(self):
        return sum(int(v) << (32*i) for i, v in enumerate(self.v))

class Point(ctypes.Structure):
    _fields_ = [("x", Fe), ("y", Fe), ("z", Fe)]


def main():
    with tempfile.TemporaryDirectory(prefix="vanitybtc-math-") as directory:
        folder = pathlib.Path(directory)
        source = folder / "check.c"
        source.write_text('''
#include <stdint.h>
typedef uint32_t uint;
typedef uint64_t ulong;
typedef uint8_t uchar;
#define __private
#define __constant const
#define __global
static void atomic_or(volatile uint *p, uint v) { *p |= v; }
#include "common.h"
Fe check_add(Fe a, Fe b) { return add(a,b); }
Fe check_sub(Fe a, Fe b) { return sub(a,b); }
Fe check_mul(Fe a, Fe b) { return mul(a,b); }
Fe check_inv(Fe a) { return inv(a); }
Point check_next(Point p) { return next_point(p); }
uint check_address(Fe x, Fe y, uint kind, uchar *out) { return encode(x,y,kind,out); }
''')
        library = folder / ("check.dylib" if sys.platform == "darwin" else "check.so")
        subprocess.run(["clang", "-std=c11", "-O2", "-shared", "-fPIC", "-I", str(ROOT / "src/gpu"), str(source), "-o", str(library)], check=True)
        lib = ctypes.CDLL(str(library))
        for name in ["add", "sub", "mul"]:
            fn = getattr(lib, "check_"+name)
            fn.argtypes, fn.restype = [Fe, Fe], Fe
        lib.check_inv.argtypes, lib.check_inv.restype = [Fe], Fe
        lib.check_next.argtypes, lib.check_next.restype = [Point], Point
        lib.check_address.argtypes, lib.check_address.restype = [Fe, Fe, ctypes.c_uint32, ctypes.c_void_p], ctypes.c_uint32
        edges = [0,1,2,2**32-1,2**32,2**128-1,2**255,P-2,P-1]
        # Reproducible public field elements, not production private keys.
        values = edges + [int.from_bytes(hashlib.sha256(i.to_bytes(4,"big")).digest(),"big") % P for i in range(2048)]
        for i, a in enumerate(values):
            b = values[-i-1]
            fa, fb = Fe.of(a), Fe.of(b)
            assert lib.check_add(fa,fb).value() == (a+b)%P
            assert lib.check_sub(fa,fb).value() == (a-b)%P
            assert lib.check_mul(fa,fb).value() == (a*b)%P
            if a and i < 100:
                assert lib.check_inv(fa).value() == pow(a,-1,P)
        for a in edges:
            for b in edges:
                assert lib.check_mul(Fe.of(a),Fe.of(b)).value() == a*b%P
                assert lib.check_add(Fe.of(a),Fe.of(b)).value() == (a+b)%P
                assert lib.check_sub(Fe.of(a),Fe.of(b)).value() == (a-b)%P
        point = Point(Fe.of(G[0]),Fe.of(G[1]),Fe.of(1))
        x, y = G
        for _ in range(128):
            zi = pow(point.z.value(),-1,P)
            assert point.x.value()*zi**2%P == x
            assert point.y.value()*zi**3%P == y
            slope = (3*x*x*pow(2*y,-1,P))%P if (x,y)==G else ((y-G[1])*pow(x-G[0],-1,P))%P
            nx = (slope*slope-x-G[0])%P
            x,y = nx,(slope*(x-nx)-y)%P
            point = lib.check_next(point)
        for kind, expected in [(0,b"1BgGZ9tcN4rm9KBzDn7KprQz87SZ26SAMH"),(1,b"bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4")]:
            out = ctypes.create_string_buffer(42)
            n = lib.check_address(Fe.of(G[0]),Fe.of(G[1]),kind,out)
            assert out.raw[:n] == expected
        print("PASS: 2,138 field pairs, 99 inversions, 128 points, and both address vectors")

if __name__ == "__main__":
    main()
