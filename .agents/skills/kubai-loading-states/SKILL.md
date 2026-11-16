---
name: kubai-loading-states
description: 逐个检查现有产品里所有需要等待的地方，判断每一处该用哪种加载方式：不显示、按钮图标转动、骨架屏、通知里的进度、乐观更新，或 AI 回答边生成边显示。先列出表格和理由，由你决定改哪些；确认后按演示效果改造，300ms 内不显示加载状态，骨架与真实布局一致，进度显示剩余时间，保存失败时原地退回并可重试。网页和手机端都适用（loading states, skeleton, optimistic UI, progress）。
---

# 加载状态的诊断与改造

《999 个小技巧，让你 Vibe 的产品更有质感》第 017 期 · @kubai087

在用户的项目里按下面的要求改造。要求以用户的口吻写成：文中的 I、me、my 指用户；写着要先问的地方，先停下来问用户再继续。

## 改造要求

Review how my product handles waiting, and decide for each wait which loading state fits: nothing, a spinner, a skeleton, progress, an optimistic change, or an AI answer written as it arrives. Then, if I want it, change them along the lines below. This is about judgment first: diagnose and recommend, and change only what I approve.

This is a change to my product, not a demo. Work inside my code, on my stack, with my components, tokens, motion values and copy style. Do not add a dependency, a data library or a new visual pattern without asking. Every value below is a default to start from.

0. Read the product first

- Find every place a person waits: page and route loads, lists and tables, search and filters, buttons that call the server (save, send, sync, delete), uploads, imports and exports, background jobs, and AI calls.
- For each one, note: what starts it; how long it usually takes (measure it, or estimate from the call: cached, one request, several, a job); what is shown meanwhile (a spinner, a skeleton, nothing, a blocking overlay); whether the layout moves when the data arrives; whether the person can keep working; what happens when it fails; and whether it can be cancelled.
- Find the shared pieces: the spinner, skeleton and progress components, the data layer (a fetch wrapper, React Query, SWR, Apollo, a store), and any global loading bar or overlay.
- Tell me in a few lines what you found before going on.

1. The kinds of waiting
   a. Under 300ms: show no separate loader (no spinner, skeleton or bar): one that appears and goes in under 300ms reads as a flicker. The one exception is a button whose own icon already means the action, which turns from the press (b). The result is the feedback: when it shows where the person is looking, nothing more is needed; when it doesn't (a sync, a copy, a save off-screen), confirm it with a short toast.
   b. A short wait for one action, length unknown (about 0.3 to 3s): show it where the person acted, with the label and the width unchanged. If the button's icon already means the action (sync, refresh), turn that icon from the press: at least one whole turn even when the answer is instant, and on until the answer lands, always stopping on a whole turn. Otherwise the icon becomes a spinner, started only after 300ms and kept at least 500ms. Block a second press while it runs, but never grey out the page. When it is done, the button goes back as it was and a short toast gives the result (Synced, 1 task updated): at the top centre, a few words, gone after about 3s.
   c. Content whose layout is known (a page, a list, a card, a table): a skeleton.

- Show what does not depend on the data at once: the title, tabs, filters, column headers.
- After 300ms, draw skeleton shapes for the rest, at the real sizes and places: the same row height and avatar size, lines a little shorter than the text, their lengths varying a little.
- A light band crosses the whole list together, every 1.2s.
- When the data arrives, the real rows take the same places, top to bottom, each fading in over 200ms, 40ms apart. Nothing jumps. If fewer rows come back, the extra shapes fade out.
- Never put a lone spinner in the middle of an empty page instead.
  d. Long work that can be counted (uploads, imports, exports, batch jobs, over about 3s): progress.
- Say what it is doing, how far (a count or a percentage, and a bar), and the time left once there is a second of data to estimate from; before that, "Estimating…".
- Show it in a notification at the top right when the work goes on while the person moves on (an import, an export, a batch job): it keeps the page free, stays on every page and is kept until handled. Work that only concerns one item can show in that item's place instead, at the same size, so nothing on the page moves.
- Keep the page usable and offer Cancel. If it can go on without the person, let it run in the background and tell them when it is done.
- Show it once the work has run 300 to 500ms; a job done sooner needs only the result.
- At the end it turns into the result in place, and opens it when clicked.
- Long work that cannot be counted: name the stage it is in (Connecting, Processing 3 files) rather than an endless spinner; after about 10s, say it is taking longer than usual and offer to keep it running in the background.
  e. An action that almost always works and is easy to reverse (mark done, like, rename, reorder, move, add a comment): optimistic.
- Show the result at once, before the server answers, everywhere it appears (the tick, the status, the totals), and save behind it. Say quietly that it is saving, in a fixed place on the item (Saving, then Saved, fading after a moment), never with a spinner over the change itself.
- If the save fails, put every part of the change back, say why on the item in a few words (Couldn’t save), and offer Retry in the place of the saving status. Keep what the person typed.
- Not for payments, deleting for good, sending something to other people, or anything that cannot be quietly put back.
  f. AI answers and other results that arrive in pieces:
- First say what it is doing, as steps (Reading 4 tasks, Checking 3 events, Writing the summary), each turning into a check.
- Then write the answer as the words arrive, each fading in from a slight blur as the next begins, with a caret after the last.
- Keep the answer's space from the start (a card at its final size, or a minimum height that only grows downward), so the page does not move as it writes.
- At the end, show Copy and Retry. Offer a way to stop only when answers often run long.
- Aim for the first words within 1 to 2s; when that is not possible, the steps must keep moving.
  g. Waits the person does not need to see (prefetching, saving a draft, syncing in the background): nothing, or a quiet status in a fixed place (Saved). Never block the page for them.

2. Rules for every kind

- Thresholds: under 300ms nothing but a button's own icon turning; 300ms to 1s a spinner or skeleton; 1 to 10s a skeleton, or progress that says what it is doing; over 10s progress, the time left and a way out. Measure on a slow mobile network, not on localhost.
- Delay and minimum: what appears after 300ms stays at least 500ms. Write one helper for this and use it everywhere.
- The layout never moves when content arrives: reserve the space (skeletons at real size, minimum heights, fixed image ratios).
- One indicator per wait: never a spinner on the button, an overlay on the page and a bar at the top all at once.
- Keep what is known: when refreshing, keep showing the old data with a quiet refresh state, rather than a skeleton.
- Failures stay where they happened, say what failed and what to do, and offer Retry. A toast that disappears is not enough for something the person has to fix.
- Reduced motion: keep every state; drop the moving band and the spin, and use a soft opacity pulse or nothing.
- Accessibility: aria-busy on the region that is loading, a polite live region for done and failed, focus left where it was, and a busy button still reachable by keyboard.

3. Report before changing anything
   Give me a table first, and do not change code yet. One row per wait:

- Where: the screen or component, and what starts the wait.
- Now: how long it takes, and what is shown.
- Should be: nothing, spinner, skeleton, progress, optimistic, AI answer, or background; and why, in a phrase that points to the rule above.
- Change: what you would do, in a phrase.
  Group the rows by kind. Then list the shared pieces you would add or change (the delay helper, skeleton shapes, the progress notification). Wait for me to choose what to change.

4. How it looks (defaults; fit them to my design)

- Spinner: the loading icon from my icon set (or a ring segment on a faint track), 12 to 16px, a turn every 0.9s, in the accent or the text colour. In a button, an icon that already means the action turns instead, speeding up over the first 18% and slowing over the last 18% so it starts and stops at rest: one turn in 640ms at the least, and on at about 600° a second for longer waits (three turns in 1.7s), always ending on a whole turn; otherwise the icon cross-fades into the spinner over 180ms with a little blur and scale. The label and width stay.
- Toast: 40px tall with a 16px corner, a check on a soft tile and a few words, top centre of the content; it comes down 10px from a slight blur over 500ms, and leaves after about 3s.
- Skeleton: shapes in the raised grey (rgb(240 240 242) on white; on dark, white at 8%), 6px corners for lines, circles for pictures, 12px lines for 14px text. The band is white at 75%, 320px wide, crossing the whole list at once.
- Progress in a notification: 344px wide, 24px corners, 14px padding (16px on the right), at the top right. A 32px icon tile centred on the words beside it, holding the file's type as a two-tone filled icon at 18px, the title (Importing tasks.csv), the count and time left in one line (568 of 1,240 rows · About 2s left), and a 4px bar in the accent on the accent at 10%; no row of buttons: the card opens what it is about, and a close mark on hover cancels. It comes down 56px onto the page out of a 4px blur and from 96% over 420ms. When it is done, the same card changes in place: the old words blur out over 160ms and the new ones in over 420ms (Imported 1,240 tasks, Added to Backlog.), the bar folds away and the tile turns solid green with a heavy white check, the same 18px. It then stays until opened or closed.
- Optimistic: a 20px checkbox in the task icon's place, shown on hover and filled green when ticked; totals roll to the new figure over 320ms; Saving with a 12px spinner, then Saved with a check, at the start of the row's last column, in 12px grey. When a save fails the row shakes 6px, twice, over 360ms, the reason in red takes the place of the row's counts, and Retry takes the place of Saving, until it is fixed.
- AI answer: steps at 13px with a 16px spinner that turns into a check; the answer at 15/24; the caret 2px wide in the accent; Retry and Copy at the end.

5. How to build it, by platform
   These are suggestions; check them against my stack and its versions, and prefer what the product already uses.

- Web: the delay and minimum as one hook or helper, for example useDelayedFlag(loading, 300, 500). React: Suspense with a skeleton fallback, startTransition to keep the old content while the new loads, useOptimistic or a mutation's onMutate with a rollback; React Query or SWR keep the previous data while they refetch. Read streamed answers from fetch with a ReadableStream, or the AI SDK's useChat. Draw the band as one gradient fixed to the list, moved by background-position, and stop it under prefers-reduced-motion.
- iOS: .redacted(reason: .placeholder) for skeletons, ProgressView for progress, Task cancellation for Cancel, optimistic changes in the view model with a rollback, AsyncSequence for streamed text.
- Android: a shimmer brush over placeholder shapes in Compose, LinearProgressIndicator with a value, Paging's LoadState for lists, optimistic state in the ViewModel with a rollback, Flow for streamed text.
- Flutter: the real widgets as placeholders under a shimmer (or Skeletonizer), LinearProgressIndicator(value:), a Stream for streamed text.
- Rust and other native desktops (egui, iced, GPUI, Tauri): the same states in the UI model, with the 300 and 500ms timers in the update loop.

6. Check

- Throttle the network (Slow 4G) and time every wait: nothing shows a separate loader under 300ms, nothing flashes, nothing moves the layout when the data comes.
- Turn the network off during an optimistic change: it goes back, says why, and Retry works.
- Cancel a long job, and leave the page during one: nothing is lost, nothing keeps spinning.
- A screen reader hears loading and done; reduced motion has no moving band; light and dark both checked; existing tests still pass.
