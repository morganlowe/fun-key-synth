#!/usr/bin/env python3
"""Generate a standalone HTML info page for a Fun Key Synth release."""

from __future__ import annotations

import argparse
import base64
import html
import mimetypes
import re
from datetime import datetime, timezone
from pathlib import Path


def embed_local_images(html_content: str, repo_root: Path) -> str:
    """Replace relative <img src="..."> paths with base64 data URIs."""
    pattern = re.compile(r'(<img\b[^>]*?\bsrc=["\'])([^"\']+)(["\'])', re.IGNORECASE)

    def _replace(match: re.Match[str]) -> str:
        prefix, src, suffix = match.groups()
        if src.startswith(("http://", "https://", "data:", "//")):
            return match.group(0)
        img_path = (repo_root / src).resolve()
        if not img_path.is_file():
            return match.group(0)
        mime_type, _ = mimetypes.guess_type(str(img_path))
        mime_type = mime_type or "application/octet-stream"
        encoded = base64.b64encode(img_path.read_bytes()).decode("ascii")
        return f"{prefix}data:{mime_type};base64,{encoded}{suffix}"

    return pattern.sub(_replace, html_content)


def render_markdown(text: str) -> str:
    """Render Markdown to HTML, falling back to preformatted text if unavailable."""
    try:
        import markdown  # type: ignore

        return markdown.markdown(
            text,
            extensions=["tables", "fenced_code", "toc", "sane_lists"],
        )
    except ImportError:
        return f"<pre>{html.escape(text)}</pre>"


def format_size(num_bytes: int) -> str:
    size = float(num_bytes)
    for unit in ("B", "KB", "MB", "GB"):
        if size < 1024.0 or unit == "GB":
            return f"{size:.1f} {unit}" if unit != "B" else f"{int(size)} B"
        size /= 1024.0
    return f"{num_bytes} B"


def build_assets_section(assets_dir: Path | None, exclude_names: set[str]) -> str:
    if not assets_dir or not assets_dir.is_dir():
        return ""
    files = sorted(
        p for p in assets_dir.iterdir()
        if p.is_file() and p.name not in exclude_names
    )
    if not files:
        return ""

    rows = "\n".join(
        f"<tr><td><code>{html.escape(p.name)}</code></td>"
        f"<td class=\"size\">{format_size(p.stat().st_size)}</td></tr>"
        for p in files
    )
    return f"""
    <section class="card">
      <h2>Files in This Folder</h2>
      <p class="muted">Download the package for your platform directly from this Google Drive folder:</p>
      <table class="assets-table">
        <thead><tr><th>File</th><th>Size</th></tr></thead>
        <tbody>
          {rows}
        </tbody>
      </table>
    </section>
    """


def main() -> None:
    parser = argparse.ArgumentParser(description="Build standalone release info HTML page.")
    parser.add_argument("--tag", required=True, help="Release tag (e.g. v1.3.0)")
    parser.add_argument("--repo", default="morganlowe/fun-key-synth", help="GitHub repo (owner/name)")
    parser.add_argument("--readme", default="README.md", help="Path to README.md")
    parser.add_argument("--release-notes", default="", help="Optional path to release notes markdown")
    parser.add_argument("--assets-dir", default="", help="Optional directory containing release files")
    parser.add_argument("--output", required=True, help="Output HTML file path")
    args = parser.parse_args()

    readme_path = Path(args.readme).resolve()
    repo_root = readme_path.parent
    readme_text = readme_path.read_text(encoding="utf-8") if readme_path.is_file() else ""

    notes_html = ""
    if args.release_notes:
        notes_path = Path(args.release_notes)
        if notes_path.is_file():
            notes_raw = notes_path.read_text(encoding="utf-8").strip()
            if notes_raw:
                notes_html = f"""
                <section class="card release-notes">
                  <h2>Release Notes — {html.escape(args.tag)}</h2>
                  {render_markdown(notes_raw)}
                </section>
                """

    output_path = Path(args.output)
    assets_dir = Path(args.assets_dir) if args.assets_dir else None
    assets_html = build_assets_section(
        assets_dir,
        exclude_names={output_path.name, "README.md", "RELEASE_NOTES.md"},
    )

    readme_html = render_markdown(readme_text)
    updated_utc = datetime.now(timezone.utc).strftime("%Y-%m-%d %H:%M UTC")
    release_url = f"https://github.com/{args.repo}/releases/tag/{args.tag}"

    page = f"""<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1.0" />
  <title>Fun Key Synth — {html.escape(args.tag)}</title>
  <style>
    :root {{
      --bg: #0f1117;
      --surface: #181c25;
      --border: #2a3142;
      --text: #e6edf3;
      --muted: #9aa6b8;
      --accent: #6ea8fe;
      --accent-2: #a371f7;
    }}
    * {{ box-sizing: border-box; }}
    body {{
      margin: 0;
      font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Inter, sans-serif;
      background: var(--bg);
      color: var(--text);
      line-height: 1.6;
    }}
    .container {{
      max-width: 960px;
      margin: 0 auto;
      padding: 2rem 1.25rem 4rem;
    }}
    .banner {{
      display: flex;
      flex-wrap: wrap;
      align-items: center;
      justify-content: space-between;
      gap: 1rem;
      background: linear-gradient(135deg, #1b2234, #221b38);
      border: 1px solid var(--border);
      border-radius: 14px;
      padding: 1.25rem 1.5rem;
      margin-bottom: 1.5rem;
    }}
    .badge {{
      display: inline-block;
      background: var(--accent-2);
      color: #fff;
      font-weight: 700;
      font-size: 0.85rem;
      padding: 0.2rem 0.65rem;
      border-radius: 999px;
      margin-right: 0.5rem;
    }}
    .btn {{
      display: inline-block;
      background: var(--accent);
      color: #0b1020;
      font-weight: 700;
      text-decoration: none;
      padding: 0.55rem 1rem;
      border-radius: 8px;
    }}
    .btn:hover {{ opacity: 0.9; }}
    .card {{
      background: var(--surface);
      border: 1px solid var(--border);
      border-radius: 14px;
      padding: 1.5rem;
      margin-bottom: 1.5rem;
    }}
    .muted {{ color: var(--muted); font-size: 0.92rem; }}
    a {{ color: var(--accent); }}
    img {{
      max-width: 100%;
      height: auto;
      border-radius: 10px;
      border: 1px solid var(--border);
    }}
    code {{
      background: #0b0e14;
      padding: 0.15rem 0.35rem;
      border-radius: 5px;
      font-size: 0.9em;
    }}
    pre {{
      background: #0b0e14;
      border: 1px solid var(--border);
      border-radius: 10px;
      padding: 1rem;
      overflow-x: auto;
    }}
    pre code {{ background: transparent; padding: 0; }}
    table {{
      width: 100%;
      border-collapse: collapse;
      margin: 1rem 0;
    }}
    th, td {{
      text-align: left;
      padding: 0.6rem 0.75rem;
      border-bottom: 1px solid var(--border);
    }}
    th {{ color: var(--muted); font-weight: 600; }}
    td.size {{ white-space: nowrap; color: var(--muted); }}
    hr {{ border: 0; border-top: 1px solid var(--border); margin: 1.5rem 0; }}
  </style>
</head>
<body>
  <div class="container">
    <div class="banner">
      <div>
        <span class="badge">Latest Release: {html.escape(args.tag)}</span>
        <span class="muted">Synced {html.escape(updated_utc)}</span>
      </div>
      <a class="btn" href="{html.escape(release_url)}" target="_blank" rel="noopener">
        View on GitHub Releases
      </a>
    </div>
    {notes_html}
    {assets_html}
    <main class="card">
      {readme_html}
    </main>
  </div>
</body>
</html>
"""

    page = embed_local_images(page, repo_root)
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(page, encoding="utf-8")
    print(f"Wrote standalone info page to {output_path} ({format_size(output_path.stat().st_size)})")


if __name__ == "__main__":
    main()
