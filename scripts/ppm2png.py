#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-or-later
# Copyright (C) 2026 João Figueiró
# Item: Raw PPM (P6) to PNG Screendump Converter
# Date: 2026-11-09
"""Converts raw PPM (P6) images to PNG using Python standard library only."""
import sys, zlib, struct

def ppm_to_png(ppm_path, png_path):
    with open(ppm_path, 'rb') as f:
        header = f.readline().strip()
        if header != b'P6':
            raise ValueError(f"Invalid PPM format: {header}")
        
        line = f.readline().strip()
        while line.startswith(b'#'):
            line = f.readline().strip()
            
        dims = line.split()
        width, height = int(dims[0]), int(dims[1])
        maxval = int(f.readline().strip())
        if maxval != 255:
            raise ValueError(f"Unsupported maximum color value: {maxval}")
            
        raw_pixels = f.read(width * height * 3)

    # Prepare scanlines for PNG (filter byte 0 per line)
    stride = width * 3
    scanlines = bytearray()
    for y in range(height):
        scanlines.append(0)  # Filter None
        scanlines.extend(raw_pixels[y * stride : (y + 1) * stride])

    def make_chunk(chunk_type, data):
        length = len(data)
        crc = zlib.crc32(chunk_type + data) & 0xffffffff
        return struct.pack('>I', length) + chunk_type + data + struct.pack('>I', crc)

    png_bytes = bytearray(b'\x89PNG\r\n\x1a\n')
    ihdr_data = struct.pack('>IIBBBBB', width, height, 8, 2, 0, 0, 0)
    png_bytes.extend(make_chunk(b'IHDR', ihdr_data))
    
    compressed_data = zlib.compress(bytes(scanlines), 9)
    png_bytes.extend(make_chunk(b'IDAT', compressed_data))
    png_bytes.extend(make_chunk(b'IEND', b''))

    with open(png_path, 'wb') as f:
        f.write(png_bytes)
    print(f"     Screenshot converted: {png_path} ({width}x{height}, {len(png_bytes)} bytes)")

if __name__ == '__main__':
    if len(sys.argv) < 3:
        print("Usage: ppm2png.py <input.ppm> <output.png>")
        sys.exit(1)
    ppm_to_png(sys.argv[1], sys.argv[2])
