import assert from "node:assert/strict";

const rows = 'nav[aria-label="AI conversations"] [data-conversation-id]';
const timeout = { timeout: 30_000 };

/** Exercise selection through real route changes and the chat action menu. */
export async function checkAiNavigation(page, clickButton, clickLink) {
  const original = new URL(page.url()).searchParams.get("sessionId");
  assert.ok(original, "The selected chat must be reflected in the URL.");
  await waitForSelected(page, original);
  const initial = await chatIds(page);
  assert.ok(initial.includes(original));

  // Leave more than one neighbor for both active and inactive deletion checks.
  for (let index = 0; index < 2; index += 1) {
    const previous = new URL(page.url()).searchParams.get("sessionId");
    await clickButton(page, "New chat");
    await page.waitForFunction(
      (id) => {
        const active = new URL(location.href).searchParams.get("sessionId");
        return active && active !== id;
      },
      timeout,
      previous,
    );
    await waitForCount(page, initial.length + index + 1);
    await waitForSelected(page, new URL(page.url()).searchParams.get("sessionId"));
    await checkModelPicker(page, clickButton);
  }
  const count = initial.length + 2;
  await selectChat(page, original);
  await clickLink(page, "Files");
  await page.waitForFunction(() => location.pathname.endsWith("/files"), timeout);
  await clickLink(page, "AI");
  await waitForSelected(page, original);
  await waitForCount(page, count);
  await page.waitForFunction(
    () =>
      document.querySelector('[role="log"]')?.textContent.includes("hello!") &&
      document.querySelector("#syntaxis-ai-composer")?.value === "Keep this unsent draft",
    timeout,
  );

  await clickButton(page, "Settings");
  await page.waitForSelector('[aria-label="AI settings"]', timeout);
  await clickButton(page, "Chat");
  await waitForSelected(page, original);
  await waitForCount(page, count);

  // A linked chat must override the remembered selection.
  const other = (await chatIds(page)).find((id) => id !== original);
  await selectChat(page, other);
  await waitForSelected(page, other);
  await selectChat(page, original);

  await openActions(page, original);
  await page.keyboard.press("Escape");
  await waitForMenusClosed(page);
  await openActions(page, original);
  await selectAction(page, "Rename chat");
  await page.waitForSelector('[role="dialog"]', timeout);
  await waitForMenusClosed(page);
  await clickButton(page, "Cancel");
  await page.waitForSelector('[role="dialog"]', { ...timeout, hidden: true });
  await waitForMenusClosed(page);

  const beforeDelete = await chatIds(page);
  await selectChat(page, beforeDelete[0]);
  await deleteChat(page, beforeDelete[0], clickButton);
  await waitForSelected(page, beforeDelete[1]);
  await waitForCount(page, count - 1);
  assert.deepEqual(await chatIds(page), beforeDelete.slice(1));

  // Removing another row must not navigate or create a chat.
  const inactive = (await chatIds(page)).find((id) => id !== beforeDelete[1]);
  await deleteChat(page, inactive, clickButton);
  await waitForSelected(page, beforeDelete[1]);
  await waitForCount(page, count - 2);

  // Keep deleting the first selected row, checking that a neighbor wins.
  let remaining = await chatIds(page);
  while (remaining.length > 1) {
    await selectChat(page, remaining[0]);
    await deleteChat(page, remaining[0], clickButton);
    await waitForSelected(page, remaining[1]);
    await waitForCount(page, remaining.length - 1);
    remaining = await chatIds(page);
  }
  const last = remaining[0];
  await deleteChat(page, last, clickButton);
  await page.waitForFunction(() => !new URL(location.href).searchParams.has("sessionId"), timeout);
  await waitForCount(page, 0);
  await page.waitForFunction(
    () => document.querySelector('[role="log"]')?.textContent.includes("Press New chat"),
    timeout,
  );
  assert.equal(await page.$("#syntaxis-ai-composer"), null);
  await clickButton(page, "Settings");
  await page.waitForSelector('[aria-label="AI settings"]', timeout);
  await clickButton(page, "Chat");
  await waitForCount(page, 0);
  await page.waitForFunction(
    () => document.querySelector('[role="log"]')?.textContent.includes("Press New chat"),
    timeout,
  );
  await clickButton(page, "New chat");
  await waitForCount(page, 1);
  const replacement = (await chatIds(page))[0];
  await waitForSelected(page, replacement);
  await checkModelPicker(page, clickButton);
  await clickLink(page, "Files");
  await page.waitForFunction(() => location.pathname.endsWith("/files"), timeout);
  await clickLink(page, "AI");
  await waitForSelected(page, replacement);
  await waitForCount(page, 1);
}

async function chatIds(page) {
  return page.$$eval(rows, (elements) => elements.map((element) => element.dataset.conversationId));
}

async function checkModelPicker(page, clickButton) {
  await page.waitForFunction(() => {
    const picker = document.querySelector('button[aria-label="Choose AI model"]');
    return picker && !picker.disabled;
  }, timeout);
  await clickButton(page, "Choose AI model");
  await page.waitForSelector('button[aria-label="Choose AI model"][aria-expanded="true"]', timeout);
  await clickButton(page, "Close model picker");
}

async function waitForCount(page, count) {
  await page.waitForFunction(
    (selector, expected) => document.querySelectorAll(selector).length === expected,
    timeout,
    rows,
    count,
  );
}

async function waitForSelected(page, id) {
  assert.ok(id, "Expected a conversation ID.");
  await page.waitForFunction(
    (selector, expected) => {
      const row = [...document.querySelectorAll(selector)].find(
        (element) => element.dataset.conversationId === expected,
      );
      const button = row?.querySelector(":scope > button");
      return (
        new URL(location.href).searchParams.get("sessionId") === expected &&
        button?.getAttribute("aria-current") === "page" &&
        !button.disabled
      );
    },
    timeout,
    rows,
    id,
  );
}

async function clickRowButton(page, id, selector) {
  const handle = await page.evaluateHandle(
    (rowsSelector, expected, buttonSelector) => {
      const row = [...document.querySelectorAll(rowsSelector)].find(
        (element) => element.dataset.conversationId === expected,
      );
      const button = row?.querySelector(buttonSelector);
      return button && !button.disabled ? button : null;
    },
    rows,
    id,
    selector,
  );
  try {
    const button = handle.asElement();
    assert.ok(button, `Chat ${id} must have an enabled ${selector}.`);
    // Use a real pointer click so keyboard actions target the focused control.
    await button.click();
  } finally {
    await handle.dispose();
  }
}

async function selectChat(page, id) {
  await clickRowButton(page, id, ":scope > button");
  await waitForSelected(page, id);
}

async function openActions(page, id) {
  await clickRowButton(page, id, 'button[aria-label^="Chat actions for "]');
  await page.waitForSelector('[role="option"]', timeout);
}

async function selectAction(page, label) {
  const clicked = await page.evaluate((text) => {
    const action = [...document.querySelectorAll('[role="option"]')].find(
      (element) => element.textContent.trim() === text,
    );
    if (!action || action.getAttribute("data-disabled") === "true") return false;
    action.click();
    return true;
  }, label);
  assert.ok(clicked, `Chat action ${label} must be available.`);
}

async function waitForMenusClosed(page) {
  await page.waitForFunction(
    () => !document.querySelector('button[aria-label^="Chat actions for "][aria-expanded="true"]'),
    timeout,
  );
}

async function deleteChat(page, id, clickButton) {
  await openActions(page, id);
  await selectAction(page, "Delete chat");
  await page.waitForSelector('[role="dialog"]', timeout);
  await waitForMenusClosed(page);
  await clickButton(page, "Delete chat");
  await page.waitForSelector('[role="dialog"]', { ...timeout, hidden: true });
  await waitForMenusClosed(page);
}
