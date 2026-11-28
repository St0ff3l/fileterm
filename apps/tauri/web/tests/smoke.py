#!/usr/bin/env python3
import argparse
import base64
import json
import os
import re
import ssl
import urllib.error
import urllib.request

from playwright.sync_api import sync_playwright


def make_opener(insecure: bool):
    if not insecure:
        return urllib.request.build_opener()
    ctx = ssl.create_default_context()
    ctx.check_hostname = False
    ctx.verify_mode = ssl.CERT_NONE
    return urllib.request.build_opener(urllib.request.HTTPSHandler(context=ctx))


def http_get(opener, url: str, headers: dict | None = None):
    req = urllib.request.Request(url, headers=headers or {}, method="GET")
    try:
        with opener.open(req, timeout=10) as resp:
            return resp.status, dict(resp.headers), resp.read()
    except urllib.error.HTTPError as err:
        return err.code, dict(err.headers), err.read()


def http_post(opener, url: str, payload: dict, headers: dict | None = None):
    all_headers = {"Content-Type": "application/json", **(headers or {})}
    req = urllib.request.Request(
        url, data=json.dumps(payload).encode("utf-8"), headers=all_headers, method="POST"
    )
    try:
        with opener.open(req, timeout=10) as resp:
            return resp.status, dict(resp.headers), resp.read()
    except urllib.error.HTTPError as err:
        return err.code, dict(err.headers), err.read()


def verify_http_guards(base_url: str, auth_header: str, insecure: bool):
    opener = make_opener(insecure)

    # /healthz works without Basic Auth
    status, _, body = http_get(opener, f"{base_url}/healthz")
    assert status == 200, f"/healthz expected 200, got {status}"
    assert json.loads(body).get("runtime") == "web"

    # Unauthenticated access is rejected with 401
    for path in ["/", "/api/bootstrap", "/api/ws", "/api/web/export-connections"]:
        status, _, _ = http_get(opener, f"{base_url}{path}")
        assert status == 401, f"unauthenticated {path} expected 401, got {status}"

    for path in ["/api/rpc", "/api/web/stage-upload"]:
        status, _, _ = http_post(opener, f"{base_url}{path}", {})
        assert status == 401, f"unauthenticated {path} expected 401, got {status}"

    # Authenticated /api/bootstrap sets HttpOnly cookie and returns Web metadata
    status, headers, body = http_get(
        opener, f"{base_url}/api/bootstrap", headers={"Authorization": auth_header}
    )
    assert status == 200, f"authenticated /api/bootstrap expected 200, got {status}"
    boot = json.loads(body)
    assert boot["isDesktop"] is False
    assert boot["platform"] == "web"
    assert boot["runtimeName"] == "Web"
    set_cookie = headers.get("Set-Cookie", "") or headers.get("set-cookie", "")
    assert "fileterm_session=" in set_cookie, f"missing session cookie: {headers}"
    assert "HttpOnly" in set_cookie

    # Cross-origin RPC POST is rejected with 403
    status, _, _ = http_post(
        opener,
        f"{base_url}/api/rpc",
        {"command": "app_get_snapshot", "args": {}},
        headers={"Authorization": auth_header, "Origin": "https://evil.example"},
    )
    assert status == 403, f"cross-origin /api/rpc expected 403, got {status}"

    # /api/ws without session cookie is rejected with 401 even when Basic Auth is sent
    status, _, _ = http_get(
        opener,
        f"{base_url}/api/ws?clientId=smoke-no-cookie",
        headers={"Authorization": auth_header},
    )
    assert status == 401, f"/api/ws without session cookie expected 401, got {status}"


def browser_launch_options():
    launch_kwargs: dict = {"headless": True}
    chrome_path = os.environ.get("FILETERM_TEST_CHROME")
    if chrome_path and os.path.exists(chrome_path):
        launch_kwargs["executable_path"] = chrome_path

    return launch_kwargs


def verify_managed_login(base_url: str, username: str, password: str, phase: str):
    with sync_playwright() as p:
        browser = p.chromium.launch(**browser_launch_options())
        context = browser.new_context(locale="en-US")
        page = context.new_page()
        page.goto(base_url)
        form = page.locator(".web-login-form")
        form.wait_for()
        rpc = f"{base_url}/api/rpc"
        assert context.request.post(rpc, data={}).status == 401

        if phase == "verify-managed-login":
            form.locator("input[type=text]").fill("admin")
            form.locator("input[type=password]").fill("admin")
            form.locator("button[type=submit]").click()
            form.locator("input[autocomplete=new-password]").wait_for()
            assert context.request.post(rpc, data={}).status == 403

            form.locator("input[type=text]").fill(username)
            form.locator("input[autocomplete=new-password]").fill(password)
            form.locator("button[type=submit]").click()
            page.wait_for_selector(".fs-shell")
            page.locator(".home-sidebar").get_by_role(
                "button", name=re.compile(r"^(Settings|设置)$")
            ).click()
            settings_items = page.locator(".settings-modal .connection-manager-sidebar-item")
            assert settings_items.first.inner_text().strip() in {"Web Login", "Web 登录"}
            settings_items.first.click()
            form.locator("input[autocomplete=current-password]").fill(password)
            form.locator("input[autocomplete=new-password]").fill(f"{password}-updated")
            form.locator("button[type=submit]").click()
            page.wait_for_selector(".web-login-form", state="hidden")
            page.wait_for_selector(".fs-shell")
            assert context.request.post(
                rpc, data={"command": "app_get_snapshot", "args": {}}
            ).status == 200
        else:
            assert context.request.post(
                f"{base_url}/api/web/login", data={"username": "admin", "password": "admin"}
            ).status == 401
            form.locator("input[type=text]").fill(username)
            form.locator("input[type=password]").fill(password)
            form.locator("button[type=submit]").click()
            page.wait_for_selector(".fs-shell")

        browser.close()


def verify_browser_ui(base_url: str, username: str, password: str, phase: str):
    with sync_playwright() as p:
        browser = p.chromium.launch(**browser_launch_options())
        context = browser.new_context(
            http_credentials={"username": username, "password": password},
            ignore_https_errors=True,
        )
        page = context.new_page()
        page_errors: list[str] = []
        console_errors: list[str] = []
        page.on("pageerror", lambda exc: page_errors.append(str(exc)))
        page.on(
            "console",
            lambda msg: console_errors.append(msg.text)
            if msg.type == "error"
            else None,
        )

        page.goto(base_url, wait_until="domcontentloaded")
        page.wait_for_selector(".fs-shell", timeout=20000)

        runtime_attr = page.evaluate("document.documentElement.dataset.runtime")
        assert runtime_attr == "web", f"expected data-runtime='web', got {runtime_attr}"

        if phase == "create-and-verify":
            # Open New Connection overlay via existing button in home workspace
            new_conn_btn = page.locator("button:has-text('新建连接'), button:has-text('New Connection')").first
            new_conn_btn.click()
            page.wait_for_selector(".web-overlay-frame .ssh-modal", timeout=10000)

            modal = page.locator(".web-overlay-frame")
            modal.locator("label:has-text('名称'), label:has-text('Name')").locator("input").first.fill(
                "smoke-persisted-host"
            )
            # Fill host & username fields inside the connection form
            host_input = modal.locator("input[placeholder*='192.168.1.10']").first
            host_input.fill("127.0.0.1")
            user_input = modal.locator("#connection-username").first
            user_input.fill("tester")
            save_btn = modal.locator("button[type='submit']").first
            save_btn.click()

            page.wait_for_selector("text=smoke-persisted-host", timeout=10000)

            # Open Local Terminal and confirm xterm renders DOM
            local_btn = page.locator("button:has-text('本地终端'), button:has-text('Local Terminal')").first
            if local_btn.count() > 0:
                local_btn.click()
                page.wait_for_selector(".xterm", timeout=10000)
        else:
            page.wait_for_selector("text=smoke-persisted-host", timeout=15000)

        assert not page_errors, f"Uncaught page errors: {page_errors}"
        assert not console_errors, f"Console errors: {console_errors}"
        browser.close()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--base-url", required=True)
    parser.add_argument("--username", required=True)
    parser.add_argument("--password", required=True)
    parser.add_argument("--insecure", action="store_true")
    parser.add_argument(
        "--phase", choices=["create-and-verify", "verify-persisted", "verify-managed-login", "verify-managed-restart"], required=True
    )
    args = parser.parse_args()

    if args.phase.startswith("verify-managed-"):
        verify_managed_login(args.base_url, args.username, args.password, args.phase)
    else:
        token = base64.b64encode(f"{args.username}:{args.password}".encode("utf-8")).decode("ascii")
        verify_http_guards(args.base_url, f"Basic {token}", args.insecure)
        verify_browser_ui(args.base_url, args.username, args.password, args.phase)
    print(f"Smoke phase '{args.phase}' passed on {args.base_url}")


if __name__ == "__main__":
    main()
