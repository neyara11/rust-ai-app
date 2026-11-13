#!/usr/bin/env python3
"""Convert a compact markdown subset to a Word .docx file.

Usage:
    docx-export --input document.md --output document.docx

Supported: headings, paragraphs, bullet/numbered lists, fenced code blocks,
simple tables, bold/italic/inline code, blockquotes. Platform anchors
(`<!-- id: ... -->`) are stripped.
"""
import argparse
import re
import sys
from pathlib import Path

try:
    from docx import Document
    from docx.shared import Pt
except ImportError:
    print("python-docx is required: pip install python-docx", file=sys.stderr)
    raise

INLINE = re.compile(r"(\*\*.+?\*\*|\*[^*]+?\*|`[^`]+?`)")


def strip_anchors(text):
    return re.sub(r"<!--.*?-->", "", text, flags=re.S)


def add_inline(paragraph, text):
    text = re.sub(r"\[([^\]]+)\]\(([^)]+)\)", r"\1 (\2)", text)
    for token in INLINE.split(text):
        if not token:
            continue
        if token.startswith("**") and token.endswith("**") and len(token) > 4:
            run = paragraph.add_run(token[2:-2])
            run.bold = True
        elif token.startswith("*") and token.endswith("*") and len(token) > 2:
            run = paragraph.add_run(token[1:-1])
            run.italic = True
        elif token.startswith("`") and token.endswith("`") and len(token) > 2:
            run = paragraph.add_run(token[1:-1])
            run.font.name = "Consolas"
            run.font.size = Pt(9)
        else:
            paragraph.add_run(token)


def heading(line):
    match = re.match(r"^(#{1,6})\s+(.*)$", line)
    if not match:
        return 0, line
    return len(match.group(1)), match.group(2).strip()


def is_bullet(line):
    return re.match(r"^\s*[-*]\s+", line) is not None


def is_numbered(line):
    return re.match(r"^\s*\d+\.\s+", line) is not None


def strip_marker(line):
    return re.sub(r"^\s*(?:[-*]|\d+\.)\s+", "", line)


def table_cells(line):
    return [cell.strip() for cell in line.strip().strip("|").split("|")]


def is_table_separator(line):
    return bool(re.match(r"^\s*\|?[\s:|-]+\|", line)) and "-" in line


def add_code(doc, lines):
    if not lines:
        return
    paragraph = doc.add_paragraph(style="No Spacing")
    run = paragraph.add_run("\n".join(lines))
    run.font.name = "Consolas"
    run.font.size = Pt(9)


def convert(markdown, output):
    doc = Document()
    lines = strip_anchors(markdown).replace("\r\n", "\n").replace("\r", "\n").split("\n")
    code = []
    in_code = False
    i = 0
    while i < len(lines):
        line = lines[i]

        if line.strip().startswith("```"):
            if in_code:
                add_code(doc, code)
                code = []
                in_code = False
            else:
                in_code = True
            i += 1
            continue
        if in_code:
            code.append(line)
            i += 1
            continue

        if (
            line.strip().startswith("|")
            and i + 1 < len(lines)
            and is_table_separator(lines[i + 1])
        ):
            header = table_cells(line)
            rows = []
            i += 2
            while i < len(lines) and lines[i].strip().startswith("|"):
                rows.append(table_cells(lines[i]))
                i += 1
            table = doc.add_table(rows=1, cols=len(header))
            table.style = "Table Grid"
            for index, cell in enumerate(header):
                table.rows[0].cells[index].text = cell
            for row in rows:
                cells = table.add_row().cells
                for index, value in enumerate(row[: len(header)]):
                    cells[index].text = value
            doc.add_paragraph()
            continue

        if not line.strip():
            i += 1
            continue
        if re.match(r"^\s*([-*_])\1{2,}\s*$", line):
            i += 1
            continue

        level, text = heading(line)
        if level:
            doc.add_heading(text, level=min(level, 6))
            i += 1
            continue
        if is_bullet(line):
            add_inline(doc.add_paragraph(style="List Bullet"), strip_marker(line))
            i += 1
            continue
        if is_numbered(line):
            add_inline(doc.add_paragraph(style="List Number"), strip_marker(line))
            i += 1
            continue
        if line.lstrip().startswith("> "):
            add_inline(doc.add_paragraph(style="Intense Quote"), line.lstrip()[2:])
            i += 1
            continue

        add_inline(doc.add_paragraph(), line)
        i += 1

    add_code(doc, code)
    doc.save(output)


def main():
    parser = argparse.ArgumentParser(description="Convert markdown to .docx")
    parser.add_argument("--input", required=True)
    parser.add_argument("--output", required=True)
    args = parser.parse_args()

    source = Path(args.input)
    output = Path(args.output)
    if not source.is_file():
        print(f"input not found: {source}", file=sys.stderr)
        return 2
    try:
        convert(source.read_text(encoding="utf-8"), output)
    except Exception as error:  # noqa: BLE001 - CLI boundary
        print(f"conversion failed: {error}", file=sys.stderr)
        return 1
    if not output.is_file():
        print("output was not created", file=sys.stderr)
        return 1
    print(f"ok: {output} ({output.stat().st_size} bytes)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
