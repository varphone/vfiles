import { describe, expect, it } from "vitest";
import { columnsFromElements, countRowColumns } from "../src/utils/gridLayout";

describe("countRowColumns", () => {
  it("counts the leading run of equal offsets", () => {
    // 3 列：前三个同一行，之后换行
    expect(countRowColumns([0, 0, 0, 120, 120, 120])).toBe(3);
    expect(countRowColumns([0, 0, 120, 120])).toBe(2);
  });

  it("handles single-row and single-item layouts", () => {
    expect(countRowColumns([0, 0, 0])).toBe(3);
    expect(countRowColumns([12])).toBe(1);
  });

  it("never returns zero, even without elements", () => {
    expect(countRowColumns([])).toBe(1);
  });

  it("tolerates sub-pixel differences", () => {
    expect(countRowColumns([0, 0.4, 0.9, 120])).toBe(3);
    expect(countRowColumns([0, 2, 4])).toBe(1);
  });
});

describe("columnsFromElements", () => {
  it("reads offsetTop from the elements", () => {
    const elements = [
      { offsetTop: 0 },
      { offsetTop: 0 },
      { offsetTop: 140 },
    ] as unknown as Element[];

    expect(columnsFromElements(elements)).toBe(2);
    expect(columnsFromElements([])).toBe(1);
  });
});
