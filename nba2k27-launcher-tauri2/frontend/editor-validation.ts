// Local web controls can hold an incomplete value before blur. Never silently
// save other fields while a displayed local value has been rejected.
export function assertDraftInputs(selector: string) {
  for (const input of document.querySelectorAll<HTMLInputElement>(
    `${selector} input[data-local-field]`,
  )) {
    if (!input.validity.valid)
      throw Error(
        `${input.getAttribute("aria-label") || "Field"}: ${input.validationMessage || "Finish or cancel this edit before saving."}`,
      );
  }
}
