import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import { renderApp, seededId } from "./utils";

async function openEquipment() {
  const user = userEvent.setup();
  const api = new MockApi();
  const id = await seededId(api, "Seraphine");
  renderApp("/equipment", { api, openCharacterId: id });
  return { user };
}

describe("inventory full-row selection", () => {
  it("selects the inventory row when a non-name cell is pressed and enables the row actions", async () => {
    // Arrange
    const { user } = await openEquipment();
    const inv = await screen.findByRole("region", { name: "Inventory" });
    const row = (await within(inv).findByRole("button", { name: /Select Explorer/ })).closest("tr")!;
    const details = within(inv).getByRole("button", { name: "Details" }) as HTMLButtonElement;
    expect(row.classList.contains("sel")).toBe(false);
    expect(details.disabled).toBe(true);

    // Act
    await user.click(row.querySelectorAll("td")[3]!);

    // Assert
    expect(row.classList.contains("sel")).toBe(true);
    expect(details.disabled).toBe(false);
  });
});
