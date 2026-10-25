import { expect, test } from "vitest";
import { shoeLabel, previewWarning } from "./shoe-labels";
import { label } from "./state";
import { readableName } from "./FitPreview";
test("catalog, outfit card and mannequin use the same verified model name", () => {
  expect(label("shoe_1000_default")).toBe("Nike Air Force 1 High · default");
  expect(readableName("shoe_1000_default")).toBe(label("shoe_1000_default"));
  expect(shoeLabel("shoe_0000_default")).toBe("Default shoes");
  expect(label("tanktop_solid_gry")).toBe("tanktop solid gry");
});
test("unmapped shoes remain identifiable without guessing a brand or blocking the outfit", () => {
  expect(shoeLabel("shoe_0215_default")).toBe("Unidentified shoe 0215 · default");
  expect(previewWarning("shoe_0215_default: no verified shoe model mapping.")).toBe("Unidentified shoe 0215 · default: 3D preview unavailable. Other clothing is still shown.");
  expect(shoeLabel("shoe_../secret")).toBeUndefined();
});
