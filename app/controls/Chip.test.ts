import { describe, it, expect } from "vitest";
import { render } from "@testing-library/svelte";
import Chip from "./Chip.svelte";

describe("Chip", () => {
    it("sets title attribute from tip prop", () => {
        const { container } = render(Chip, {
            props: { context: false, target: false, tip: "Tooltip text" },
        });
        let chip = container.querySelector(".chip");
        expect(chip?.getAttribute("title")).toBe("Tooltip text");
    });
});
