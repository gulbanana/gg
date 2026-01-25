import { describe, it, expect } from "vitest";
import { sameChange } from "./ids";

describe("sameChange", () => {
    it("returns true for identical change ids", () => {
        let a = { hex: "abc123", prefix: "abc", offset: 0 };
        let b = { hex: "abc123", prefix: "abc", offset: 0 };
        expect(sameChange(a, b)).toBe(true);
    });

    it("returns false when hex differs", () => {
        let a = { hex: "abc123", prefix: "abc", offset: 0 };
        let b = { hex: "def456", prefix: "def", offset: 0 };
        expect(sameChange(a, b)).toBe(false);
    });

    it("returns false when offset differs (divergent changes)", () => {
        let a = { hex: "abc123", prefix: "abc", offset: 0 };
        let b = { hex: "abc123", prefix: "abc", offset: 1 };
        expect(sameChange(a, b)).toBe(false);
    });

    it("ignores prefix differences", () => {
        let a = { hex: "abc123", prefix: "abc", offset: 0 };
        let b = { hex: "abc123", prefix: "ab", offset: 0 };
        expect(sameChange(a, b)).toBe(true);
    });
});
