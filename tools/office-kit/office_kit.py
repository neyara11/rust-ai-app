import argparse
import json
import re
import sys
import zipfile
from datetime import date, datetime, time
from pathlib import Path

from docx import Document
from docx.enum.text import WD_ALIGN_PARAGRAPH
from docx.oxml.ns import qn
from docx.shared import Cm, Inches, Pt
from docx.table import Table
from docx.text.paragraph import Paragraph

MAX_SHEET_ROWS = 200
MAX_SHEET_COLS = 40
MAX_IMAGE_INCHES = 6.2


def fail(message, code=1):
    print(message, file=sys.stderr)
    raise SystemExit(code)


def read_text(path):
    return path.read_text(encoding="utf-8-sig")


def write_text(path, text):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8", newline="\n")


def safe_name(raw, fallback="Лист"):
    name = re.sub(r"[\[\]:*?/\\]", " ", str(raw or "")).strip()
    name = re.sub(r"\s+", " ", name)
    return (name or fallback)[:31]


def md_cell(value):
    if value is None:
        return ""
    if isinstance(value, bool):
        return "да" if value else "нет"
    if isinstance(value, (datetime, date, time)):
        return value.isoformat()
    text = str(value)
    text = text.replace("\r\n", "\n").replace("\r", "\n").replace("\n", "<br>")
    return text.replace("|", "\\|").strip()


def md_table(rows):
    if not rows:
        return []
    width = max(len(row) for row in rows)
    padded = [list(row) + [""] * (width - len(row)) for row in rows]
    header = padded[0]
    lines = ["| " + " | ".join(md_cell(cell) for cell in header) + " |"]
    lines.append("| " + " | ".join("---" for _ in header) + " |")
    for row in padded[1:]:
        lines.append("| " + " | ".join(md_cell(cell) for cell in row) + " |")
    return lines


def run_markdown(text):
    parts = []
    for token in re.split(r"(\*\*.+?\*\*|\*[^*]+?\*|`[^`]+?`)", text):
        if not token:
            continue
        if token.startswith("**") and token.endswith("**") and len(token) > 4:
            parts.append("**" + token[2:-2] + "**")
        elif token.startswith("*") and token.endswith("*") and len(token) > 2:
            parts.append("*" + token[1:-1] + "*")
        elif token.startswith("`") and token.endswith("`") and len(token) > 2:
            parts.append("`" + token[1:-1] + "`")
        else:
            parts.append(token)
    return "".join(parts)


def paragraph_markdown(paragraph):
    runs = list(paragraph.runs)
    if not runs:
        return paragraph.text.strip()
    out = []
    for run in runs:
        text = run.text
        if not text:
            continue
        code = (run.font.name or "").lower() in {"consolas", "courier new", "cascadia mono"}
        if code:
            text = "`" + text + "`"
        if run.bold and text.strip():
            text = "**" + text.strip() + "**" + (" " if text.endswith(" ") else "")
        elif run.italic and text.strip():
            text = "*" + text.strip() + "*" + (" " if text.endswith(" ") else "")
        out.append(text)
    return "".join(out).strip()


def save_part(part, assets_dir, counter):
    suffix = Path(str(part.partname)).suffix.lower() or ".bin"
    if suffix == ".jpe":
        suffix = ".jpg"
    counter[0] += 1
    name = f"image-{counter[0]}{suffix}"
    target = assets_dir / name
    target.write_bytes(part.blob)
    return name


def paragraph_images(paragraph, document, assets_dir, counter):
    names = []
    blips = paragraph._p.xpath(".//a:blip")
    for blip in blips:
        rid = blip.get(qn("r:embed"))
        if not rid:
            continue
        part = document.part.related_parts.get(rid)
        if part is None or not hasattr(part, "blob"):
            continue
        names.append(save_part(part, assets_dir, counter))
    return names


def embedded_files(path, assets_dir, out_path, depth):
    lines = []
    try:
        archive = zipfile.ZipFile(path)
    except zipfile.BadZipFile:
        return lines
    max_files = 20
    max_bytes = 50 * 1024 * 1024
    seen = 0
    with archive:
        for name in archive.namelist():
            if not re.match(r"^(word|xl)/embeddings/.+\.(docx|xlsx)$", name, re.IGNORECASE):
                continue
            if seen >= max_files:
                lines.append(f"- вложенных файлов больше {max_files}, остальные пропущены")
                break
            info = archive.getinfo(name)
            if info.file_size > max_bytes:
                lines.append(f"- вложенный файл {Path(name).name} больше 50 МБ, пропущен")
                continue
            data = archive.read(name)
            seen += 1
            base = Path(name).name
            target = assets_dir / base
            target.write_bytes(data)
            rel = f"{assets_dir.name}/{base}"
            lines.append(f"- вложенный файл: [{base}]({rel})")
            if depth < 2:
                try:
                    nested_md = assets_dir / (Path(base).stem + ".md")
                    read_office(target, nested_md, depth + 1)
                    lines.append(
                        f"- вложенный файл разобран: [{nested_md.name}]({assets_dir.name}/{nested_md.name})"
                    )
                except SystemExit:
                    lines.append(f"- вложенный файл {base} не разобран")
    return lines


def read_docx(path, out_path, depth=0):
    document = Document(str(path))
    assets_dir = out_path.parent / (out_path.stem + ".assets")
    assets_dir.mkdir(parents=True, exist_ok=True)
    counter = [0]
    lines = []
    props = document.core_properties
    if props.title:
        lines.append(f"> Название: {props.title.strip()}")
    if props.author:
        lines.append(f"> Автор: {props.author.strip()}")
    if lines:
        lines.append("")

    tables = 0
    images = 0
    for block in document.iter_inner_content():
        if isinstance(block, Paragraph):
            style = (block.style.name or "").lower()
            text = paragraph_markdown(block)
            images_here = paragraph_images(block, document, assets_dir, counter)
            images += len(images_here)
            if text:
                tight = False
                if style.startswith("heading"):
                    if lines and re.match(r"^(?:- |\d+\. )", lines[-1]):
                        lines.append("")
                    level = re.sub(r"\D", "", style) or "1"
                    lines.append("#" * min(int(level or 1), 6) + " " + text)
                elif style == "title":
                    lines.append("# " + text)
                elif "list bullet" in style:
                    lines.append("- " + text)
                    tight = True
                elif "list number" in style:
                    lines.append("1. " + text)
                    tight = True
                elif "quote" in style:
                    lines.append("> " + text)
                else:
                    lines.append(text)
                if not tight:
                    lines.append("")
            for name in images_here:
                lines.append(f"![Изображение]({assets_dir.name}/{name})")
                lines.append("")
        elif isinstance(block, Table):
            rows = []
            for row in block.rows:
                rows.append([cell.text for cell in row.cells])
            if rows:
                tables += 1
                lines.extend(md_table(rows))
                lines.append("")

    nested = embedded_files(path, assets_dir, out_path, depth)
    if nested:
        lines.append("## Вложенные файлы")
        lines.extend(nested)
        lines.append("")

    text = "\n".join(lines).strip() + "\n"
    if not text.strip():
        text = "_Документ не содержит текста._\n"
    write_text(out_path, text)
    return {
        "kind": "docx",
        "paragraphs": len(document.paragraphs),
        "tables": tables,
        "images": images,
        "assets": str(assets_dir),
        "bytes": out_path.stat().st_size,
    }


def chart_title(chart):
    try:
        title = chart.title
        if not title or not title.tx or not title.tx.rich or not title.tx.rich.p:
            return ""
        return "".join(run.t or "" for run in title.tx.rich.p[0].r).strip()
    except Exception:
        return ""


def xlsx_images(ws, assets_dir, sheet_index, counter):
    names = []
    for image in getattr(ws, "_images", []):
        try:
            if hasattr(image, "_data"):
                data = image._data()
            else:
                ref = image.ref
                data = ref.read() if hasattr(ref, "read") else ref.getvalue()
            if isinstance(data, str):
                data = data.encode("utf-8")
            fmt = (getattr(image, "format", None) or "png").lower()
            counter[0] += 1
            name = f"sheet{sheet_index}-image-{counter[0]}.{fmt}"
            (assets_dir / name).write_bytes(data)
            names.append(name)
        except Exception:
            continue
    return names


def read_xlsx(path, out_path, depth=0):
    import openpyxl

    workbook = openpyxl.load_workbook(filename=str(path), data_only=False, read_only=False)
    assets_dir = out_path.parent / (out_path.stem + ".assets")
    assets_dir.mkdir(parents=True, exist_ok=True)
    counter = [0]
    lines = []
    if workbook.properties and workbook.properties.title:
        lines.append(f"> Название: {workbook.properties.title.strip()}")
        lines.append("")
    sheets = []
    truncated_any = False
    total_images = 0
    total_charts = 0
    for index, ws in enumerate(workbook.worksheets, start=1):
        max_row = ws.max_row or 0
        max_col = ws.max_column or 0
        capped_rows = min(max_row, MAX_SHEET_ROWS)
        capped_cols = min(max_col, MAX_SHEET_COLS)
        truncated = max_row > MAX_SHEET_ROWS or max_col > MAX_SHEET_COLS
        truncated_any = truncated_any or truncated
        lines.append(f"## Лист: {ws.title}")
        note = []
        if ws.sheet_state != "visible":
            note.append(f"состояние: {ws.sheet_state}")
        note.append(f"размер: {max_row}×{max_col}")
        if truncated:
            note.append(f"показаны первые {capped_rows} строк и {capped_cols} столбцов")
        if ws.merged_cells.ranges:
            note.append("объединения: " + ", ".join(str(rng) for rng in ws.merged_cells.ranges))
        lines.append("> " + "; ".join(note))
        lines.append("")

        rows = []
        for row in ws.iter_rows(min_row=1, max_row=capped_rows, max_col=capped_cols):
            rows.append([cell.value for cell in row])
        while rows and all(value is None for value in rows[-1]):
            rows.pop()
        if rows:
            lines.extend(md_table(rows))
        else:
            lines.append("_Лист пуст._")
        lines.append("")

        images = xlsx_images(ws, assets_dir, index, counter)
        total_images += len(images)
        for name in images:
            lines.append(f"![Изображение]({assets_dir.name}/{name})")
            lines.append("")
        charts = getattr(ws, "_charts", [])
        total_charts += len(charts)
        if charts:
            lines.append("### Диаграммы")
            for chart in charts:
                title = chart_title(chart)
                kind = type(chart).__name__
                lines.append(f"- {kind}" + (f": {title}" if title else ""))
            lines.append("")

        sheets.append({"name": ws.title, "rows": max_row, "cols": max_col, "truncated": truncated})

    nested = embedded_files(path, assets_dir, out_path, depth)
    if nested:
        lines.append("## Вложенные файлы")
        lines.extend(nested)
        lines.append("")

    text = "\n".join(lines).strip() + "\n"
    if not text.strip():
        text = "_Книга не содержит данных._\n"
    write_text(out_path, text)
    return {
        "kind": "xlsx",
        "sheets": sheets,
        "images": total_images,
        "charts": total_charts,
        "truncated": truncated_any,
        "assets": str(assets_dir),
        "bytes": out_path.stat().st_size,
    }


def read_office(path, out_path, depth=0):
    suffix = path.suffix.lower()
    if suffix == ".docx":
        return read_docx(path, out_path, depth)
    if suffix == ".xlsx":
        return read_xlsx(path, out_path, depth)
    fail(f"unsupported input: {suffix or path.name} (only .docx and .xlsx)", 2)


INLINE = re.compile(r"(\*\*.+?\*\*|\*[^*]+?\*|`[^`]+?`|\[[^\]]+\]\([^)]+\))")
IMAGE_LINE = re.compile(r"^!\[(.*?)\]\((\S+?)(?:\s+\"[^\"]*\")?\)\s*$")


def add_inline(paragraph, text):
    for token in INLINE.split(text):
        if not token:
            continue
        link = re.fullmatch(r"\[([^\]]+)\]\(([^)]+)\)", token)
        if link:
            run = paragraph.add_run(link.group(1))
            run.underline = True
            if not link.group(1).strip().endswith(link.group(2)):
                paragraph.add_run(f" ({link.group(2)})")
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


def is_table_separator(line):
    return bool(re.match(r"^\s*\|?[\s:|-]+\|", line)) and "-" in line


def table_cells(line):
    return [cell.strip() for cell in line.strip().strip("|").split("|")]


def resolve_image(raw, base_dir):
    if re.match(r"^[a-zA-Z]:[\\/]", raw) or raw.startswith(("/", "\\")):
        raise ValueError("absolute image paths are not allowed")
    candidate = (base_dir / raw).resolve()
    root = base_dir.resolve()
    if root not in candidate.parents and candidate != root:
        raise ValueError(f"image path escapes the working folder: {raw}")
    if not candidate.is_file():
        raise ValueError(f"image not found: {raw}")
    return candidate


def image_width_px(path):
    try:
        from PIL import Image

        with Image.open(path) as image:
            return image.size[0]
    except Exception:
        return 0


def export_docx(source, output):
    document = Document()
    for section in document.sections:
        section.page_width = Cm(21)
        section.page_height = Cm(29.7)
        section.left_margin = Cm(2)
        section.right_margin = Cm(2)
        section.top_margin = Cm(2)
        section.bottom_margin = Cm(2)
    normal = document.styles["Normal"]
    normal.font.name = "Calibri"
    normal.font.size = Pt(11)
    normal.element.rPr.rFonts.set(qn("w:eastAsia"), "Calibri")

    text = re.sub(r"<!--.*?-->", "", read_text(source), flags=re.S)
    lines = text.replace("\r\n", "\n").replace("\r", "\n").split("\n")
    base_dir = source.parent
    code = []
    in_code = False
    i = 0
    images = 0
    tables = 0
    while i < len(lines):
        line = lines[i]
        if line.strip().startswith("```"):
            if in_code:
                if code:
                    paragraph = document.add_paragraph(style="No Spacing")
                    run = paragraph.add_run("\n".join(code))
                    run.font.name = "Consolas"
                    run.font.size = Pt(9)
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

        image = IMAGE_LINE.match(line.strip())
        if image:
            alt, raw = image.group(1), image.group(2)
            try:
                path = resolve_image(raw, base_dir)
            except ValueError as error:
                fail(str(error))
            px = image_width_px(path)
            width = Inches(MAX_IMAGE_INCHES)
            if px:
                natural = Inches(px / 96.0)
                width = natural if natural < width else width
            document.add_picture(str(path), width=width)
            document.paragraphs[-1].alignment = WD_ALIGN_PARAGRAPH.CENTER
            if alt and alt.lower() not in {"изображение", "image"}:
                caption = document.add_paragraph(alt)
                caption.alignment = WD_ALIGN_PARAGRAPH.CENTER
                for run in caption.runs:
                    run.italic = True
                    run.font.size = Pt(9)
            images += 1
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
            table = document.add_table(rows=1, cols=len(header))
            table.style = "Table Grid"
            for index, cell in enumerate(header):
                table.rows[0].cells[index].text = cell
                for run in table.rows[0].cells[index].paragraphs[0].runs:
                    run.bold = True
            for row in rows:
                cells = table.add_row().cells
                for index, value in enumerate(row[: len(header)]):
                    cells[index].text = value
            document.add_paragraph()
            tables += 1
            continue

        if not line.strip():
            i += 1
            continue
        if re.match(r"^\s*([-*_])\1{2,}\s*$", line):
            i += 1
            continue

        level, title = heading(line)
        if level:
            document.add_heading(title, level=min(level, 6))
            if level == 1 and not document.core_properties.title:
                document.core_properties.title = title
            i += 1
            continue
        if re.match(r"^\s*[-*]\s+", line):
            add_inline(document.add_paragraph(style="List Bullet"), re.sub(r"^\s*[-*]\s+", "", line))
            i += 1
            continue
        if re.match(r"^\s*\d+\.\s+", line):
            add_inline(document.add_paragraph(style="List Number"), re.sub(r"^\s*\d+\.\s+", "", line))
            i += 1
            continue
        if line.lstrip().startswith("> "):
            add_inline(document.add_paragraph(style="Intense Quote"), line.lstrip()[2:])
            i += 1
            continue

        add_inline(document.add_paragraph(), line)
        i += 1

    if code:
        paragraph = document.add_paragraph(style="No Spacing")
        run = paragraph.add_run("\n".join(code))
        run.font.name = "Consolas"
        run.font.size = Pt(9)
    document.save(str(output))
    return {
        "kind": "docx",
        "images": images,
        "tables": tables,
        "bytes": output.stat().st_size,
    }


def parse_markdown_tables(text):
    lines = text.replace("\r\n", "\n").replace("\r", "\n").split("\n")
    blocks = []
    sheet = None
    current = []
    i = 0
    while i < len(lines):
        line = lines[i]
        heading_match = re.match(r"^(#{1,3})\s+(.*)$", line)
        if heading_match and not line.strip().startswith("|"):
            if current:
                blocks.append((sheet, current))
                current = []
            if len(heading_match.group(1)) <= 2:
                sheet = heading_match.group(2).strip()
            i += 1
            continue
        if line.strip().startswith("|") and i + 1 < len(lines) and is_table_separator(lines[i + 1]):
            rows = [table_cells(line)]
            i += 2
            while i < len(lines) and lines[i].strip().startswith("|"):
                rows.append(table_cells(lines[i]))
                i += 1
            blocks.append((sheet, rows))
            continue
        i += 1
    return blocks


def apply_header_style(ws, columns):
    from openpyxl.styles import Alignment, Border, Font, PatternFill, Side

    fill = PatternFill("solid", fgColor="DDEBF7")
    side = Side(style="thin", color="B6C2CF")
    border = Border(left=side, right=side, top=side, bottom=side)
    for index in range(1, columns + 1):
        cell = ws.cell(row=1, column=index)
        cell.font = Font(bold=True)
        cell.fill = fill
        cell.border = border
        cell.alignment = Alignment(vertical="center", wrap_text=True)


def autosize(ws, widths=None):
    from openpyxl.utils import get_column_letter

    if widths:
        for index, width in enumerate(widths, start=1):
            ws.column_dimensions[get_column_letter(index)].width = float(width)
        return
    for column in ws.columns:
        length = 0
        letter = None
        for cell in column:
            if letter is None:
                letter = cell.column_letter
            value = "" if cell.value is None else str(cell.value)
            length = max(length, max((len(part) for part in value.split("\n")), default=0))
        if letter:
            ws.column_dimensions[letter].width = min(max(10, length + 2), 48)


def parse_range(ws, raw):
    from openpyxl.utils.cell import range_boundaries

    match = re.fullmatch(r"([A-Za-z]+)(\d+)(?::([A-Za-z]+)(\d+))?", str(raw or "").strip())
    if not match:
        raise ValueError(f"invalid range: {raw}")
    if match.group(3):
        return range_boundaries(f"{match.group(1)}{match.group(2)}:{match.group(3)}{match.group(4)}")
    return range_boundaries(f"{match.group(1)}{match.group(2)}")


def add_chart(ws, spec, base_dir):
    from openpyxl.chart import (
        AreaChart,
        BarChart,
        DoughnutChart,
        LineChart,
        PieChart,
        RadarChart,
        Reference,
        ScatterChart,
        Series,
    )

    kind = (spec.get("type") or "bar").lower()
    chart_map = {
        "bar": BarChart,
        "column": BarChart,
        "line": LineChart,
        "area": AreaChart,
        "pie": PieChart,
        "doughnut": DoughnutChart,
        "radar": RadarChart,
        "scatter": ScatterChart,
    }
    if kind not in chart_map:
        raise ValueError(f"unsupported chart type: {kind}")
    chart = chart_map[kind]()
    chart.title = spec.get("title") or None
    if spec.get("height"):
        chart.height = float(spec["height"])
    if spec.get("width"):
        chart.width = float(spec["width"])
    if kind in {"bar", "column"}:
        chart.type = "bar" if spec.get("horizontal") else "col"
        chart.style = 10
    categories = spec.get("categories")
    for series_spec in spec.get("series", []):
        name = series_spec.get("name")
        if kind == "scatter":
            x = parse_range(ws, series_spec.get("x") or categories)
            y = parse_range(ws, series_spec.get("y") or series_spec.get("values"))
            series = Series(Reference(ws, min_col=y[0], min_row=y[1], max_col=y[2], max_row=y[3]), title=name)
            series.xvalues = Reference(ws, min_col=x[0], min_row=x[1], max_col=x[2], max_row=x[3])
        else:
            bounds = parse_range(ws, series_spec.get("values"))
            series = Series(
                Reference(ws, min_col=bounds[0], min_row=bounds[1], max_col=bounds[2], max_row=bounds[3]),
                title=name,
            )
            if categories:
                cat = parse_range(ws, series_spec.get("categories") or categories)
                series.categories = Reference(ws, min_col=cat[0], min_row=cat[1], max_col=cat[2], max_row=cat[3])
        chart.series.append(series)
    anchor = spec.get("anchor") or "H2"
    ws.add_chart(chart, anchor)


def coerce_value(value):
    if isinstance(value, str):
        raw = value.strip()
        if re.fullmatch(r"-?(0|[1-9]\d{0,14})(\.\d+)?", raw):
            try:
                return int(raw) if re.fullmatch(r"-?\d+", raw) else float(raw)
            except ValueError:
                return value
    return value


def build_workbook(spec, source_dir, output):
    from openpyxl import Workbook
    from openpyxl.styles import Alignment

    workbook = Workbook()
    workbook.remove(workbook.active)
    if spec.get("title"):
        workbook.properties.title = str(spec["title"])
    sheets = spec.get("sheets") or []
    if not sheets:
        raise ValueError("workbook has no sheets")
    used = set()
    for sheet_spec in sheets:
        name = safe_name(sheet_spec.get("name"))
        base = name
        counter = 2
        while name in used:
            name = safe_name(f"{base} {counter}")
            counter += 1
        used.add(name)
        ws = workbook.create_sheet(title=name)
        columns = [str(c) for c in sheet_spec.get("columns") or []]
        if columns:
            for index, value in enumerate(columns, start=1):
                ws.cell(row=1, column=index, value=value)
        start_row = 1 if columns else 0
        for row_index, row in enumerate(sheet_spec.get("rows") or [], start=start_row + 1):
            for col_index, value in enumerate(row, start=1):
                cell = ws.cell(row=row_index, column=col_index)
                if isinstance(value, str) and value.startswith("="):
                    cell.value = value
                elif value is None:
                    continue
                else:
                    cell.value = coerce_value(value)
                cell.alignment = Alignment(vertical="top", wrap_text=True)
        if columns:
            apply_header_style(ws, len(columns))
            ws.freeze_panes = sheet_spec.get("freeze") or "A2"
        for address, fmt in (sheet_spec.get("formats") or {}).items():
            ws[address].number_format = str(fmt)
        for merged in sheet_spec.get("merges") or []:
            ws.merge_cells(str(merged))
        if sheet_spec.get("autofilter"):
            ws.auto_filter.ref = str(sheet_spec["autofilter"])
        autosize(ws, sheet_spec.get("widths"))
        if sheet_spec.get("images"):
            from openpyxl.drawing.image import Image as XLImage

            for image_spec in sheet_spec["images"]:
                raw = image_spec.get("path")
                if not raw:
                    continue
                path = resolve_image(raw, source_dir)
                picture = XLImage(str(path))
                if image_spec.get("width"):
                    picture.width = int(float(image_spec["width"]) * 96)
                if image_spec.get("height"):
                    picture.height = int(float(image_spec["height"]) * 96)
                ws.add_image(picture, image_spec.get("anchor") or "A1")
        for chart_spec in sheet_spec.get("charts") or []:
            add_chart(ws, chart_spec, source_dir)
    if not workbook.worksheets:
        raise ValueError("workbook has no sheets")
    workbook.active = 0
    workbook.save(str(output))


def export_xlsx(source, output):
    if source.suffix.lower() == ".json":
        spec = json.loads(read_text(source))
        build_workbook(spec, source.parent, output)
        return {"kind": "xlsx", "mode": "json", "sheets": len(spec.get("sheets") or [])}

    blocks = parse_markdown_tables(read_text(source))
    sheets = []
    used = set()
    text = read_text(source)
    title_match = re.search(r"^#\s+(.+)$", text, re.M)
    title = title_match.group(1).strip() if title_match else ""
    from openpyxl.utils import get_column_letter

    for sheet_name, rows in blocks:
        if not rows:
            continue
        name = safe_name(sheet_name or title or "Лист")
        base = name
        counter = 2
        while name in used:
            name = safe_name(f"{base} {counter}")
            counter += 1
        used.add(name)
        last_col = get_column_letter(max(1, min(len(rows[0]), 26)))
        sheets.append(
            {
                "name": name,
                "columns": rows[0],
                "rows": rows[1:],
                "freeze": "A2",
                "autofilter": f"A1:{last_col}1",
            }
        )
    if not sheets:
        raise ValueError("no markdown tables found; nothing to export")
    build_workbook({"title": title, "sheets": sheets}, source.parent, output)
    return {"kind": "xlsx", "mode": "markdown", "sheets": len(sheets)}


def main():
    for stream in (sys.stdout, sys.stderr):
        try:
            stream.reconfigure(encoding="utf-8")
        except Exception:
            pass
    parser = argparse.ArgumentParser(description="Office tools for office-writer")
    parser.add_argument("command", choices=["read", "docx", "xlsx"])
    parser.add_argument("--input", required=True)
    parser.add_argument("--output", required=True)
    args = parser.parse_args()

    source = Path(args.input)
    output = Path(args.output)
    if not source.is_file():
        fail(f"input not found: {source}", 2)
    output.parent.mkdir(parents=True, exist_ok=True)

    try:
        if args.command == "read":
            if output.suffix.lower() != ".md":
                fail("read: output must be a .md file", 2)
            stats = read_office(source, output)
        elif args.command == "docx":
            stats = export_docx(source, output)
        else:
            stats = export_xlsx(source, output)
    except SystemExit:
        raise
    except Exception as error:
        fail(f"{args.command} failed: {error}")

    print(json.dumps({"ok": True, "output": str(output), **stats}, ensure_ascii=False))


if __name__ == "__main__":
    main()
