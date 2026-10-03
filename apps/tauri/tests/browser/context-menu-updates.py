"""Run after bundling fixtures/context-menu-updates.tsx with esbuild to a temporary directory.
Usage: python context-menu-updates.py /absolute/path/to/bundle.js
"""
import sys
from pathlib import Path
from playwright.sync_api import sync_playwright

bundle = Path(sys.argv[1])
with sync_playwright() as playwright:
    browser = playwright.chromium.launch(headless=True, channel="chrome")
    try:
        page = browser.new_page(viewport={"width": 1000, "height": 760})
        errors = []
        page.on("pageerror", lambda error: errors.append(str(error)))
        page.on("console", lambda message: errors.append(message.text) if message.type == "error" else None)
        page.set_content('<html data-theme="fileterm-light"><body><main id="root"></main></body></html>')
        page.add_style_tag(path=str(bundle.with_suffix(".css")))
        page.add_script_tag(path=str(bundle))
        terminal = page.get_by_role("textbox", name="terminal")
        terminal.wait_for()
        terminal.focus()
        for x, y in [(100, 100), (995, 755)]:
            page.evaluate("window.commits = 0")
            page.evaluate("([x,y]) => window.open(x,y)", [x, y])
            menu = page.get_by_role("menu")
            menu.wait_for()
            assert page.evaluate("window.commits") == 1, "opening/clamping a menu must not schedule a layout commit"
            # Inspect the rendered menu before exercising keyboard and focus.
            assert menu.get_by_role("menuitem").count() > 5
            first = menu.get_by_role("menuitem").nth(1)
            first.focus()
            page.evaluate("window.commits = 0")
            for _ in range(50):
                page.evaluate("window.update()")
            assert page.evaluate("window.commits") == 50, "unchanged coordinates must not add layout commits"
            assert first.evaluate("el => document.activeElement === el"), "prop updates must not restore terminal focus"
            bounds = menu.bounding_box()
            assert bounds["x"] >= 8 and bounds["y"] >= 8
            assert bounds["x"] + bounds["width"] <= 992
            assert bounds["y"] + bounds["height"] <= 752
            page.keyboard.press("Escape")
            menu.wait_for(state="detached")
            assert terminal.evaluate("el => document.activeElement === el")
        assert page.evaluate("window.closeRevision") == 100, "event listener must use the latest callback"
        assert not errors, errors
        print("PASS: 100 terminal-menu prop updates, bounded commits, edge placement, focus and latest Escape callback")
    finally:
        browser.close()
