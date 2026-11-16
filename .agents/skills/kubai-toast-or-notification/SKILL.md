---
name: kubai-toast-or-notification
description: 逐条检查现有产品里的提示，判断每一条该用 Toast（状态、带撤销按钮、进行中）、通知，还是都不用（放在字段旁、留在原处、操作前确认）。先列出表格和理由，由你决定改哪些；可选按演示效果改造 Toast（模糊入场、完成时扫光、倒数撤销、原地变为结果）和通知（全局挂起、堆叠、悬停展开、逐条关闭和全部清除）。网页和手机端都适用（toast, snackbar, notification center, undo）。
---

# Toast 与通知的选择与改造

《999 个小技巧，让你 Vibe 的产品更有质感》第 016 期 · @kubai087

在用户的项目里按下面的要求改造。要求以用户的口吻写成：文中的 I、me、my 指用户；写着要先问的地方，先停下来问用户再继续。

## 改造要求

Review how my product tells people that something happened, and decide for each message whether it should be a toast, a notification, or neither. Then, if I want it, restyle the toasts and notifications along the lines below. This is about judgment first: diagnose and recommend, and change only what I approve.

This is a change to my product, not a demo. Work inside my code, on my stack, with my components, tokens, corner scale, fonts and copy style. Do not add a dependency, a new state library or a new visual pattern without asking. Every value below is a default to start from.

0. Read the product first

- Find every place that shows a message about an event: toasts and snackbars, alerts and banners, notification lists, inbox or activity panels, badges and counts, inline success and error text, and browser or push notifications.
- For each one, note: what triggers it (the person's own action, another person, the system, a background job), what it says, any buttons, where it appears, how long it stays, and whether it can be found again later.
- Find the components behind them and where they are mounted (one per page, or one for the whole app). Note how they announce themselves to screen readers.
- Tell me in a few lines what you found before going on.

1. What each one is for

- Toast: answers something the person just did, in the place they are working. It is short, it does not interrupt, and it goes away by itself. It comes in three kinds:
  a. Status: confirms a result, so the person does not have to go and check. An icon and a few words, no buttons. Example: "Link copied".
  b. Action: the action has already been done, and the toast holds the one step that may follow, usually Undo. One line and one button, plus a close. It stays long enough to use the button, with the time left shown.
  c. Progress: work that takes a moment. It shows that the work is under way, then turns into the result in the same place, with a button to see it. Example: "Uploading brand-guide.pdf" turns into "Uploaded to Files, Open".
- Notification: news the person did not cause just now, from other people or from the system: a mention, a reply, a payment, a deadline, a security alert, a finished report. It can carry more: a title, a few lines and up to two actions. It is shown on every page, stays until it is opened, handled or closed, and several can be shown at once. It is also kept somewhere the person can find it again (an inbox or a list behind a bell, with a count).
- Neither:
  - A form error or a problem with one field: show it next to the field.
  - A failure the person has to fix (an upload that failed, a payment that did not go through): keep it on the item, with a way to fix it, until it is fixed. A toast that disappears after a few seconds is not enough.
  - An action that cannot be undone: ask before doing it (a confirm next to the button, or a dialog for something with a large impact). A toast after the fact is too late.
  - Something only worth seeing when the person looks: a badge, a dot or a change of state in place, with no message at all.

2. Questions that settle a case

- Who caused it? The person, just now: toast. Someone else, the system, or the person a while ago in another place: notification.
- Does the person need to act? No: status toast. One quick step that may follow (Undo, View): action toast. A real decision or a reply: notification. Something that must be fixed: keep it on the item.
- Can the action be undone? Then do it at once and offer Undo in a toast instead of asking first. If it cannot be undone, confirm before, not after.
- How much is there to say? A few words: toast. A sentence or two, a name, an amount, a place: notification.
- Would it matter if the person missed it? No: toast. Yes: notification, kept until handled.
- Is the work still running when the person leaves? If it ends while they are still on the page, a toast is enough. If they have moved to another tab or app, a browser or system notification (with permission) is the only way to reach them; inside the product, it also goes into the notification list.
- Several at once? Toasts: one at a time; a new one replaces the old one, or waits. Notifications: they stack.

3. Report before changing anything
   Give me a table first, and do not change code yet. One row per message:

- Where: the screen or component, and what triggers it.
- Now: how it is shown today, in a few words.
- Should be: status toast, action toast, progress toast, notification, or neither (and which).
- Why: one line, using the questions above.
- Change: what would change (kind, wording, buttons, how long it stays, where it is kept).
  Mark the ones that are already right. Mark the ones that could go either way and say what decides between them, as a question for me. Then list anything missing: an action that should offer Undo, a failure that only shows a toast, a background job that never says it is done, news with no place to find it again.
  Then ask me which rows to change and wait.

4. Optional: restyle the toast and the notifications
   After the table, offer these as a separate choice, and wait for my answer:
   A. Restyle the toast only.
   B. Restyle the notifications only.
   C. Both.
   D. Neither; just fix the kinds and wording.
   For each one I pick, show me a short before and after (values and motion), in my product's own colours and corners, and wait for my go-ahead before building.

4A. Toast

- Place: the top centre of the page, 12 under the top bar, over the content. One at a time.
- Shape: 40 tall, 16 corners (or the nearest step in my corner scale), a white surface with a soft top-to-bottom gradient (#ffffff to #f7f9fc), a 1px border rgba(214, 219, 227, 0.95), and a soft shadow (0 16px 36px rgba(17, 24, 39, 0.08), 0 3px 10px rgba(17, 24, 39, 0.05)). Inner shapes are concentric with it: the icon, 8 in, is 24 with 8 corners; a button or the close, 6 in, is 28 with 10 corners.
- Content: icon, then one line of 14/20 text, then the button and the close if there are any. The done icon is a check on a soft tile in my accent colour; work in progress is a ring; the time left to undo is a ring that empties, with the seconds in its centre. The button is accent-coloured text, with a tint only on hover. Keep the gaps tight: 8 between the icon and the text, the button and the close next to each other.
- Enter: in 500ms on cubic-bezier(0.25, 0.1, 0.25, 1), the whole toast comes down from 10px above, grows from 96% and clears from an 8px blur; it is fully opaque a little before the end. Text and controls move with it as one piece, not letter by letter.
- Leave: 220ms, ease-in: up 6px, blur 6px, fade out.
- Sheen: a light band crosses it once (900ms, ease-out) only when something is done: a status toast as it enters, a progress toast as it turns into its result, an Undo toast after Undo. An action toast that still waits for a choice enters without it.
- Change in place (progress to result, archived to restored): the old line (text, button and close together) blurs out and moves 5px up in 180ms; the new line blurs in from 5px below between 100 and 400ms; the toast takes its new width in 380ms on cubic-bezier(0.32, 0.72, 0, 1). Only the icon has a move of its own: the old one shrinks and blurs away while the new one turns in from -40 degrees and half size.
- After Undo: the change is reverted, and the toast says so ("Task restored") for about 1.5 seconds before it leaves.
- Timing: a status toast stays 3 to 5 seconds, longer for longer text; an action toast at least 5 seconds; a progress toast stays while the work runs, then 3 seconds. The timer pauses while the pointer is over the toast or focus is inside it.

4B. Notifications

- Place: the top right, 12 under the top bar, the same on every page, above the content. 344 wide.
- Card: 24 corners with 12 of padding, so the picture (32, a person's photo or the product's logo) and the buttons (32 tall, 12 corners) are concentric with it. A title (14/20, semibold) with the time on the right, up to three lines of 13/20 text, then up to two buttons on the bottom right, the main one last.
- A stack: the newest is in front. The older ones are cut to the front one's height and peek out 12 below it, at 95% and 90%, showing only their edge; at most three show.
- A new one arrives: it slides down onto the stack from 56px above in 560ms on cubic-bezier(0.32, 0.72, 0, 1), clearing from a light blur, while the others are pushed down and back a step. The one it covers keeps its content until the new one is on its way over it, so it reads as stacking, not replacing.
- Hover: the stack spreads into a list at the cards' own heights, 8 apart, in 380ms on the same curve. In the list each card keeps to a hairline and a light shadow, so no shadow falls on the card below; stacked, only the front card has the deep shadow.
- Close one, as on macOS: a small close button (24, 10 corners) appears over a card's top-left corner while the pointer is on that card. Handling a card with one of its buttons also removes it. A removed card slides 48px to the right and fades in 260ms, and the ones below close the gap in 400ms. "Clear all" sits under the last card while the list is spread out.
- Counts: the bell shows how many are waiting, and so does the page title, so a tab in the background shows it too. Notifications that need a decision (security, payments) stay in the list behind the bell even when they are cleared from the screen.

4C. For both

- Reduced motion: keep the change of state and a short fade (about 150ms); drop the movement, scaling and blur.
- Screen readers: toasts are announced politely (role="status"); a notification that needs attention is announced once as it arrives. Neither takes focus away from what the person is doing. Their buttons can be reached by keyboard, and Escape closes the one in focus.
- Touch: the same places, with swipe to dismiss; there is no hover, so the stack spreads on tap.
- Dark mode: the same rules on my dark surfaces; the sheen is fainter.

5. Build what I pick

- List the files you will touch and wait for my go-ahead.
- Change one thing at a time and keep what each action does. Then check:
  - Short and long text, a missing picture, many notifications at once, several toasts in a row.
  - Undo used, ignored and closed; a progress that fails; leaving the page while something is still running.
  - Phone and desktop layouts, light and dark, reduced motion, keyboard and screen reader.
  - Existing tests still pass.
- Web: CSS transitions or the Web Animations API for the toast; for the stack, measure each card's height and animate transform and height together. Sonner (React) has a similar stack and can be adapted, but ask before adding it. iOS, Android and Flutter: use the platform's own animation tools with the same values; for messages that reach people outside the app, use the system's notifications.
