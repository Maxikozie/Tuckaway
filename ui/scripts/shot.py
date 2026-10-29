"""Screenshots of the running app (WebView2 with --remote-debugging-port=9333).

    py -3.14 ui/scripts/shot.py out_dir name [WxH ...] [--do "js"] [--hover sel] [--click sel] [--key K]

Steps run in order, then one screenshot per size. Sizes default to 1280x800.
"""
import pathlib
import sys

from playwright.sync_api import sync_playwright

out = pathlib.Path(sys.argv[1])
out.mkdir(parents=True, exist_ok=True)
name = sys.argv[2]
args = sys.argv[3:]
sizes, steps = [], []
i = 0
while i < len(args):
    a = args[i]
    if a.startswith("--"):
        steps.append((a[2:], args[i + 1]))
        i += 2
    else:
        w, h = map(int, a.split("x"))
        sizes.append((w, h))
        i += 1
sizes = sizes or [(1280, 800)]

with sync_playwright() as p:
    b = p.chromium.connect_over_cdp("http://127.0.0.1:9333")
    page = next(pg for c in b.contexts for pg in c.pages if "localhost" in pg.url or "tauri" in pg.url)
    for w, h in sizes:
        page.set_viewport_size({"width": w, "height": h})
        page.wait_for_timeout(350)
        for kind, val in steps:
            if kind == "do":
                page.evaluate(val)
            elif kind == "hover":
                page.locator(val).first.hover()
            elif kind == "click":
                page.locator(val).first.click()
            elif kind == "key":
                page.keyboard.press(val)
            elif kind == "type":
                page.keyboard.type(val)
            elif kind == "wait":
                page.wait_for_timeout(int(val))
            page.wait_for_timeout(260)
        f = out / f"{name}-{w}x{h}.png"
        page.screenshot(path=str(f))
        print(f)
    errs = page.evaluate("window.__errs || []")
    if errs:
        print("errors:", errs)
