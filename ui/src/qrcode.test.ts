import { describe, expect, it } from 'vitest';
import { encodeQrMatrix, generateQrSvg } from './qrcode';

describe('qrcode generator', () => {
  it('generates a valid square matrix for short input', () => {
    const matrix = encodeQrMatrix('127.0.0.1:25565');
    expect(matrix.length).toBeGreaterThanOrEqual(21);
    expect(matrix[0].length).toBe(matrix.length);

    // Verify finder pattern top-left
    expect(matrix[0][0]).toBe(true);
    expect(matrix[0][6]).toBe(true);
    expect(matrix[6][0]).toBe(true);
    expect(matrix[6][6]).toBe(true);
    expect(matrix[3][3]).toBe(true); // center
    expect(matrix[1][1]).toBe(false); // white ring
  });

  it('generates a valid square matrix for longer input', () => {
    const matrix = encodeQrMatrix('play.hyperserver-example.org:25565');
    expect(matrix.length).toBeGreaterThan(21);
    expect(matrix[0].length).toBe(matrix.length);
  });

  it('generates SVG string with valid dimensions and markup', () => {
    const svg = generateQrSvg('192.168.1.100:25565');
    expect(svg).toContain('<svg');
    expect(svg).toContain('</svg>');
    expect(svg).toContain('viewBox=');
    expect(svg).toContain('<rect');
  });
});
