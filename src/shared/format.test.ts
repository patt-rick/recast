import { baseName, formatBytes, formatEta } from "./format";

test("formatBytes", () => {
  expect(formatBytes(500)).toBe("500 B");
  expect(formatBytes(1536)).toBe("1.5 KB");
  expect(formatBytes(25 * 1024 * 1024)).toBe("25.0 MB");
  expect(formatBytes(150 * 1024 * 1024)).toBe("150 MB");
});

test("formatEta", () => {
  expect(formatEta(null)).toBe("");
  expect(formatEta(42)).toBe("42s left");
  expect(formatEta(125)).toBe("2m 05s left");
  expect(formatEta(3700)).toBe("1h 01m left");
});

test("baseName", () => {
  expect(baseName("C:\\v\\holiday.mp3")).toBe("holiday.mp3");
  expect(baseName("C:/v/a b.png")).toBe("a b.png");
});
