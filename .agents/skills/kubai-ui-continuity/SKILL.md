---
name: kubai-ui-continuity
description: 诊断现有产品（手机和电脑都适用）里哪些地方是整屏直接切换，找出值得做连续性的场景：列表进入详情、卡片展开、迷你播放条展开、搜索、数据卡片展开成图表、按钮展开成菜单、标签切换和布局变化。先按使用频率和改造成本排序给出建议，你选定后再按现有技术栈改造，让点到的元素直接变成下一个状态（UI continuity, shared element transition, View Transitions）。
---

# 界面连续性诊断与改造

《999 个小技巧，让你 Vibe 的产品更有质感》第 015 期 · @kubai087

在用户的项目里按下面的要求改造。要求以用户的口吻写成：文中的 I、me、my 指用户；写着要先问的地方，先停下来问用户再继续。

## 改造要求

Find the places in my product where the interface jumps from one state to the next, and make the important ones continuous: the element I touched should carry into what comes next instead of disappearing and being replaced. First diagnose and suggest; build only what I pick.

This is a change to my product, not a demo. Work inside my code, on my stack, with my components, tokens and layout. Keep what every action does, what each screen shows and where things end up; change only how one state becomes the next. Do not copy a demo scene (no phone frame, backdrop, sample content or fonts). Do not add a dependency or a router change without asking. Every value below is a default to start from. It applies to phone and desktop layouts alike; section 3 says where they differ.

1. Read the product first

- Find the screens, routes, panels, sheets, dialogs, menus and lists, and how one opens from another: navigation stack or router, modal and sheet presenters, popover and menu components, list and grid components.
- Find what animates today and how: CSS transitions, the View Transitions API, Motion or Framer Motion, GSAP, SwiftUI or UIKit transitions, Compose animations, Flutter Hero or implicit animations. Note the motion tokens (durations, curves, springs) and reduced-motion handling, and reuse them.
- Note which layouts exist (phone, tablet, desktop window, split view) and the input on each (touch, mouse, keyboard).
- Tell me in a few lines what you found before going on.

1. What to look for
   A jump is where I act on one element and the next state appears from nowhere: a hard cut, a cross-fade of whole screens, or something sliding in from an unrelated edge. Look for these, the most common first:
   a. A row or item opens its detail page. Continuous: the row's title grows into the page title; its picture into the header; the previous page's title shrinks into the back button. When the page scrolls, the large title tucks into the top bar, centred on the control row. Going back reverses all of it.
   b. A card opens into a full view or a dialog. Continuous: the card itself grows into the view (its frame, corners and picture), its title stays the same line of type and only scales, and the rest fades in once the card is mostly open. Closing, or dragging it down, shrinks it back into the same card in the same place.
   c. A compact bar opens into a full panel: a mini player, a chat bubble, an upload or download pill, a toast with details. Continuous: the bar is the panel; its cover, title and main button grow into their places in it. The panel stays opaque all the way, and the page behind steps back (scale about 0.94, dimmed about 30%).
   d. Search or filter. Continuous: the field moves up into the top row as the page title leaves and a Cancel or close slides in; while typing, the matching items slide from where they are to their place in the results and the rest fade out. On desktop the same applies to a command palette opening from its field.
   e. A summary opens into its detail: a metric tile into a chart, a progress ring into a report, a thumbnail into a gallery. Continuous: the small marks become the large ones (the sparkline is the chart's line, the small bars are the chart's bars), the label moves into the title, and the number scales up rather than being redrawn.
   f. A control opens what it controls: a ⋯ or + button into its menu or form, a select into its list, a date field into its picker. Continuous: the panel grows out of the control from the control's own corner, its items fade in after it is mostly open (about 25ms apart), and the highlight slides from item to item with the pointer or finger. When an action adds, removes or moves something, it appears in its place and the others move to make room; nothing jumps.
   g. Tabs, segmented controls, bottom bars and side navigation. Continuous: one indicator travels from the old tab to the new one. Its leading edge leaves first and the trailing edge follows, so it stretches on the way and settles as it lands (a small squash of about 15% of its height at the longest is enough); labels collapse and expand while the other items make room. New content enters from the side of the tab that was chosen.
   h. Layout changes: grid to list, sort, reorder, insert, delete, expand and collapse. Continuous: each item moves from where it was to where it goes; items that change row leave at the end of their row and come in at the start of the next, so nothing crosses the grid.

1. How to make it continuous

- One element, not two. The thing that moves is the same element (or a shared element the framework matches by id), measured where it starts and where it ends. Animate between the two boxes with transforms; never animate width, height, top or margin on things that reflow.
- What only one state has fades: content that exists only at the end fades in after about 45-55% of the move, and content that exists only at the start fades out in the first 25-35%. Text that changes size scales; it is not cross-faded with a copy.
- Keep layers honest: a panel that grows is opaque all the way, so nothing shows through it; the moving element stays above its neighbours while it moves, and goes back to its place in the stack when it lands.
- Shapes keep their corners: the radius changes smoothly between start and end, using the product's own corner curve. Nested shapes keep equal padding and concentric corners.
- Timing: open and close with a fast start and a long settle, cubic-bezier(.32, .72, 0, 1) or the product's own spring. Phones: 450-700ms for something that becomes a full screen, 350-500ms for menus and sheets, 400-450ms for an indicator. Desktop: about two thirds of that, and shorter still when it was triggered from the keyboard. Moves within a screen use ease-in-out. Staggers stay between 20 and 50ms.
- Gestures: while a finger or the pointer drags, follow it 1:1 (a dragged card shrinks to about 0.82 as it goes); on release, finish with the release velocity, toward whichever end is nearer.
- Interruptions: a new action takes over from wherever the animation is; nothing waits for an animation to finish, and input is never blocked.
- Reduced motion: keep the change of state and a short cross-fade (about 150ms), drop the movement and scaling.
- Accessibility: focus moves to the new state's heading or first control, and back to the element that opened it when it closes; screen readers announce the new state as they do now.

1. Phone and desktop

- Phone: everything above, with gestures (swipe back, drag to dismiss) driving the same animation as the buttons, and safe areas respected at every frame.
- Desktop: the same patterns, smaller and quicker. A card or row usually opens into a side panel or dialog rather than a full window; grow it from the card into that panel. Do not zoom whole windows. Hover never starts a layout animation. Keep pointer targets in place while they animate, so a second click lands where it aims. In split views, animate inside the pane that changes.
- Window resizing and responsive breakpoints are not transitions; do not animate them.

1. Report before building
   Go through the product and give me a ranked list, at most 10 entries, each with:

- Where: the screen or component, and the action that triggers it (on phone, desktop or both).
- Now: what happens today, in one line.
- Pattern: which of a-h it is, and which element should carry over.
- Change: what moves to where, the duration and curve, in two or three lines.
- Value: high, medium or low, from how often people do it and how lost they could get without it.
- Cost: what has to change in the code, and any risk (routing, virtualized lists, shared state).
  Put the frequent, disorienting ones with low cost first. Then ask me which ones to build and wait.

1. Build what I pick

- Describe the approach for each on my platform (section 6 is a starting point), and list the files you will touch. Wait for my go-ahead.
- Build them one at a time. For each, list the files you touched, and check:
  - The first and last frame match the real states exactly; nothing jumps when the moving element hands over to the real one.
  - Real data: long and short text, missing images, the first and last item, an empty list, a scrolled list.
  - Fast repeated taps or clicks, going back during the animation, and gestures cancelled half way.
  - Phone and desktop layouts, light and dark, reduced motion, keyboard and screen reader.
  - At rest every screen looks exactly as before, and existing tests still pass.

1. How, by platform (check against my versions)

- Web: the View Transitions API with view-transition-name on the shared elements (same-document, and cross-document where pages are separate), with a plain update as the fallback. Otherwise FLIP: measure first, apply the change, measure last, animate the inverse with transforms (Motion's layout and layoutId do this; so do Svelte's flip and crossfade, and Vue's TransitionGroup). For router-driven pages, start the transition from the router's navigation hook.
- iOS: SwiftUI matchedGeometryEffect for shared elements in one view, navigationTransition(.zoom(sourceID:in:)) with matchedTransitionSource on iOS 18 and later for pushes and sheets; UIKit preferredTransition = .zoom on iOS 18, or a custom UIViewControllerAnimatedTransitioning with UIViewPropertyAnimator for interruptible, gesture-driven ones.
- Android: Compose SharedTransitionLayout with Modifier.sharedElement and sharedBounds, AnimatedContent, Modifier.animateItem for lists, and predictive back for gesture-driven returns; with Views, MaterialContainerTransform and shared element transitions.
- Flutter: Hero for shared elements between routes, OpenContainer from the animations package for card to page, AnimatedList and implicit animations for layout changes.
- Desktop apps: Electron and Tauri take the web route (Tauri uses the system WebView, so test View Transitions there and keep the fallback); native apps use the same ideas with their own animation tools.
