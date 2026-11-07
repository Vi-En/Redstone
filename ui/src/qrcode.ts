// Lightweight QR Code Generator (Versions 1-4, Byte mode, Error Correction Level L)

const EXP = new Uint8Array(512);
const LOG = new Uint8Array(256);
let val = 1;
for (let i = 0; i < 255; i++) {
  EXP[i] = val;
  EXP[i + 255] = val;
  LOG[val] = i;
  val = (val << 1) ^ (val >= 128 ? 0x11d : 0);
}

function gfMul(x: number, y: number): number {
  if (x === 0 || y === 0) return 0;
  return EXP[LOG[x] + LOG[y]];
}

function rsGenPoly(degree: number): Uint8Array {
  let poly = new Uint8Array([1]);
  for (let i = 0; i < degree; i++) {
    const next = new Uint8Array(poly.length + 1);
    for (let j = 0; j < poly.length; j++) {
      next[j] ^= gfMul(poly[j], EXP[i]);
      next[j + 1] ^= poly[j];
    }
    poly = next;
  }
  return poly;
}

function rsCompute(data: Uint8Array, eccCount: number): Uint8Array {
  const gen = rsGenPoly(eccCount);
  const res = new Uint8Array(eccCount);
  for (let i = 0; i < data.length; i++) {
    const factor = data[i] ^ res[0];
    for (let j = 0; j < eccCount - 1; j++) {
      res[j] = res[j + 1] ^ gfMul(gen[eccCount - 1 - j], factor);
    }
    res[eccCount - 1] = gfMul(gen[0], factor);
  }
  return res;
}

interface VersionSpec {
  version: number;
  size: number;
  totalCodewords: number;
  dataCodewords: number;
  eccCodewords: number;
  alignmentPos?: number;
}

const VERSIONS: VersionSpec[] = [
  { version: 1, size: 21, totalCodewords: 26, dataCodewords: 19, eccCodewords: 7 },
  { version: 2, size: 25, totalCodewords: 44, dataCodewords: 34, eccCodewords: 10, alignmentPos: 18 },
  { version: 3, size: 29, totalCodewords: 70, dataCodewords: 55, eccCodewords: 15, alignmentPos: 22 },
  { version: 4, size: 33, totalCodewords: 100, dataCodewords: 80, eccCodewords: 20, alignmentPos: 26 }
];

export function encodeQrMatrix(text: string): boolean[][] {
  const utf8Bytes = new TextEncoder().encode(text);
  const spec = VERSIONS.find((v) => utf8Bytes.length <= v.dataCodewords - 3) ?? VERSIONS[VERSIONS.length - 1];

  // Bit buffer
  const bits: number[] = [];
  const pushBits = (num: number, count: number) => {
    for (let i = count - 1; i >= 0; i--) {
      bits.push((num >>> i) & 1);
    }
  };

  // Mode: byte (0100)
  pushBits(0b0100, 4);
  // Character count indicator: 8 bits
  pushBits(utf8Bytes.length, 8);
  // Data bytes
  for (const byte of utf8Bytes) {
    pushBits(byte, 8);
  }
  // Terminator: up to 4 bits of 0
  const totalDataBits = spec.dataCodewords * 8;
  const termLen = Math.min(4, totalDataBits - bits.length);
  for (let i = 0; i < termLen; i++) bits.push(0);

  // Pad to byte boundary
  while (bits.length % 8 !== 0) bits.push(0);

  // Pad bytes 0xEC and 0x11
  const dataBytes: number[] = [];
  for (let i = 0; i < bits.length; i += 8) {
    let b = 0;
    for (let j = 0; j < 8; j++) b = (b << 1) | bits[i + j];
    dataBytes.push(b);
  }
  const pad = [0xec, 0x11];
  let pIdx = 0;
  while (dataBytes.length < spec.dataCodewords) {
    dataBytes.push(pad[pIdx % 2]);
    pIdx++;
  }

  // Error correction
  const ecc = rsCompute(new Uint8Array(dataBytes), spec.eccCodewords);
  const allCodewords = [...dataBytes, ...Array.from(ecc)];

  // Convert to bit stream
  const allBits: number[] = [];
  for (const cw of allCodewords) {
    for (let i = 7; i >= 0; i--) {
      allBits.push((cw >>> i) & 1);
    }
  }

  // Initialize matrix and function reservation mask
  const size = spec.size;
  const matrix: boolean[][] = Array.from({ length: size }, () => Array(size).fill(false));
  const isFunction: boolean[][] = Array.from({ length: size }, () => Array(size).fill(false));

  const setModule = (r: number, c: number, val: boolean) => {
    if (r >= 0 && r < size && c >= 0 && c < size) {
      matrix[r][c] = val;
      isFunction[r][c] = true;
    }
  };

  // Place Finder Pattern
  const placeFinder = (row: number, col: number) => {
    for (let r = -1; r <= 7; r++) {
      for (let c = -1; c <= 7; c++) {
        const nr = row + r;
        const nc = col + c;
        if (nr < 0 || nr >= size || nc < 0 || nc >= size) continue;
        const inOuter = r >= 0 && r <= 6 && c >= 0 && c <= 6;
        const inBorder = inOuter && (r === 0 || r === 6 || c === 0 || c === 6);
        const inCenter = inOuter && r >= 2 && r <= 4 && c >= 2 && c <= 4;
        setModule(nr, nc, inBorder || inCenter);
      }
    }
  };

  placeFinder(0, 0);
  placeFinder(0, size - 7);
  placeFinder(size - 7, 0);

  // Timing patterns
  for (let i = 8; i < size - 8; i++) {
    const val = i % 2 === 0;
    if (!isFunction[6][i]) setModule(6, i, val);
    if (!isFunction[i][6]) setModule(i, 6, val);
  }

  // Alignment pattern
  if (spec.alignmentPos) {
    const pos = spec.alignmentPos;
    for (let r = -2; r <= 2; r++) {
      for (let c = -2; c <= 2; c++) {
        const isCenter = r === 0 && c === 0;
        const isBorder = Math.abs(r) === 2 || Math.abs(c) === 2;
        setModule(pos + r, pos + c, isCenter || isBorder);
      }
    }
  }

  // Dark module
  setModule(size - 8, 8, true);

  // Reserve format info area
  for (let i = 0; i <= 8; i++) {
    if (!isFunction[8][i]) isFunction[8][i] = true;
    if (!isFunction[i][8]) isFunction[i][8] = true;
  }
  for (let i = size - 8; i < size; i++) {
    if (!isFunction[8][i]) isFunction[8][i] = true;
    if (!isFunction[i][8]) isFunction[i][8] = true;
  }

  // Place data with Mask 0 ((r + c) % 2 == 0)
  let bitIdx = 0;
  let upwards = true;
  for (let right = size - 1; right > 0; right -= 2) {
    const col = right === 6 ? right - 1 : right;
    const rows = upwards
      ? Array.from({ length: size }, (_, i) => size - 1 - i)
      : Array.from({ length: size }, (_, i) => i);

    for (const r of rows) {
      for (const c of [col, col - 1]) {
        if (!isFunction[r][c]) {
          const bit = bitIdx < allBits.length ? allBits[bitIdx++] : 0;
          const mask = (r + c) % 2 === 0;
          matrix[r][c] = (bit ^ (mask ? 1 : 0)) === 1;
        }
      }
    }
    upwards = !upwards;
  }

  // Format bits (Level L, Mask 0 = 0x77C4 = 0b111011111000100)
  const formatBits = 0x77c4;
  for (let i = 0; i < 15; i++) {
    const bit = ((formatBits >>> i) & 1) === 1;
    // Top-left
    if (i < 6) matrix[8][i] = bit;
    else if (i === 6) matrix[8][7] = bit;
    else if (i === 7) matrix[8][8] = bit;
    else if (i === 8) matrix[7][8] = bit;
    else matrix[14 - i][8] = bit;

    // Split format bits around corners
    if (i < 8) matrix[size - 1 - i][8] = bit;
    else matrix[8][size - 15 + i] = bit;
  }

  return matrix;
}

export function generateQrSvg(text: string, scale = 6, margin = 4): string {
  const matrix = encodeQrMatrix(text);
  const size = matrix.length;
  const viewBoxSize = size + margin * 2;
  const rects: string[] = [];

  for (let r = 0; r < size; r++) {
    for (let c = 0; c < size; c++) {
      if (matrix[r][c]) {
        rects.push(`<rect x="${c + margin}" y="${r + margin}" width="1" height="1" fill="#16231f" />`);
      }
    }
  }

  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${viewBoxSize} ${viewBoxSize}" width="${viewBoxSize * scale}" height="${viewBoxSize * scale}" shape-rendering="crispEdges"><rect width="100%" height="100%" fill="#ffffff" />${rects.join('')}</svg>`;
}
