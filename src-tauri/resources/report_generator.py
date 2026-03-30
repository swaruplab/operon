#!/usr/bin/env python3
"""
Operon Report Generator — Professional PDF report renderer.

Usage:
    python3 report_generator.py <config.json> <output.pdf>

Reads a ReportConfig JSON file and generates a formatted PDF using reportlab.
Falls back to fpdf2 if reportlab is not available.
"""

import json
import sys
import os
from datetime import datetime

def install_if_missing(package, pip_name=None):
    """Try to import, install if missing."""
    try:
        __import__(package)
    except ImportError:
        import subprocess
        subprocess.check_call([
            sys.executable, "-m", "pip", "install", "--quiet",
            "--break-system-packages", pip_name or package
        ])

# Ensure we have reportlab
install_if_missing("reportlab")

from reportlab.lib import colors
from reportlab.lib.pagesizes import letter
from reportlab.lib.styles import getSampleStyleSheet, ParagraphStyle
from reportlab.lib.units import inch, mm
from reportlab.lib.enums import TA_LEFT, TA_CENTER, TA_JUSTIFY, TA_RIGHT
from reportlab.platypus import (
    SimpleDocTemplate, Paragraph, Spacer, Table, TableStyle,
    PageBreak, Image, KeepTogether, HRFlowable, ListFlowable, ListItem
)
from reportlab.platypus.tableofcontents import TableOfContents
from reportlab.lib.colors import HexColor

# ── Color Palette ──
DARK_BLUE = HexColor("#1a365d")
MEDIUM_BLUE = HexColor("#2b6cb0")
LIGHT_BLUE = HexColor("#ebf8ff")
DARK_GRAY = HexColor("#2d3748")
MEDIUM_GRAY = HexColor("#4a5568")
LIGHT_GRAY = HexColor("#edf2f7")
ACCENT = HexColor("#3182ce")
TABLE_HEADER_BG = HexColor("#2b6cb0")
TABLE_ALT_ROW = HexColor("#f7fafc")
CITATION_COLOR = HexColor("#2b6cb0")

# ── Styles ──

def get_styles():
    styles = getSampleStyleSheet()

    styles.add(ParagraphStyle(
        name='ReportTitle',
        parent=styles['Title'],
        fontSize=22,
        leading=28,
        textColor=DARK_BLUE,
        alignment=TA_CENTER,
        spaceAfter=6,
        fontName='Helvetica-Bold',
    ))

    styles.add(ParagraphStyle(
        name='ReportSubtitle',
        parent=styles['Normal'],
        fontSize=11,
        leading=14,
        textColor=MEDIUM_GRAY,
        alignment=TA_CENTER,
        spaceAfter=20,
    ))

    styles.add(ParagraphStyle(
        name='SectionHeading',
        parent=styles['Heading1'],
        fontSize=15,
        leading=20,
        textColor=DARK_BLUE,
        spaceBefore=18,
        spaceAfter=8,
        fontName='Helvetica-Bold',
        borderWidth=0,
        borderPadding=0,
    ))

    styles.add(ParagraphStyle(
        name='SubHeading',
        parent=styles['Heading2'],
        fontSize=12,
        leading=16,
        textColor=MEDIUM_BLUE,
        spaceBefore=12,
        spaceAfter=6,
        fontName='Helvetica-Bold',
    ))

    styles.add(ParagraphStyle(
        name='BodyText2',
        parent=styles['Normal'],
        fontSize=10.5,
        leading=15,
        textColor=DARK_GRAY,
        alignment=TA_JUSTIFY,
        spaceAfter=8,
        fontName='Helvetica',
    ))

    styles.add(ParagraphStyle(
        name='Abstract',
        parent=styles['Normal'],
        fontSize=10,
        leading=14,
        textColor=DARK_GRAY,
        alignment=TA_JUSTIFY,
        spaceAfter=6,
        fontName='Helvetica-Oblique',
        leftIndent=20,
        rightIndent=20,
    ))

    styles.add(ParagraphStyle(
        name='Caption',
        parent=styles['Normal'],
        fontSize=9,
        leading=12,
        textColor=MEDIUM_GRAY,
        alignment=TA_CENTER,
        spaceBefore=4,
        spaceAfter=12,
        fontName='Helvetica-Oblique',
    ))

    styles.add(ParagraphStyle(
        name='Reference',
        parent=styles['Normal'],
        fontSize=9,
        leading=12,
        textColor=DARK_GRAY,
        spaceAfter=4,
        leftIndent=20,
        firstLineIndent=-20,
        fontName='Helvetica',
    ))

    styles.add(ParagraphStyle(
        name='ToolTable',
        parent=styles['Normal'],
        fontSize=9,
        leading=12,
        textColor=DARK_GRAY,
        fontName='Courier',
    ))

    styles.add(ParagraphStyle(
        name='Footer',
        parent=styles['Normal'],
        fontSize=8,
        textColor=MEDIUM_GRAY,
        alignment=TA_CENTER,
    ))

    return styles


def add_header_footer(canvas, doc, title="", date=""):
    """Add page header and footer."""
    canvas.saveState()

    # Header line
    canvas.setStrokeColor(LIGHT_BLUE)
    canvas.setLineWidth(1.5)
    canvas.line(0.75*inch, letter[1] - 0.6*inch, letter[0] - 0.75*inch, letter[1] - 0.6*inch)

    # Header text
    canvas.setFont('Helvetica', 7)
    canvas.setFillColor(MEDIUM_GRAY)
    if title:
        canvas.drawString(0.75*inch, letter[1] - 0.5*inch, title[:80])
    if date:
        canvas.drawRightString(letter[0] - 0.75*inch, letter[1] - 0.5*inch, date)

    # Footer line
    canvas.setStrokeColor(LIGHT_GRAY)
    canvas.line(0.75*inch, 0.6*inch, letter[0] - 0.75*inch, 0.6*inch)

    # Page number
    canvas.setFont('Helvetica', 8)
    canvas.setFillColor(MEDIUM_GRAY)
    canvas.drawCentredString(letter[0] / 2, 0.4*inch, f"Page {doc.page}")

    # Generated by line
    canvas.setFont('Helvetica', 6)
    canvas.setFillColor(colors.Color(0.7, 0.7, 0.7))
    canvas.drawRightString(letter[0] - 0.75*inch, 0.4*inch, "Generated by Operon IDE")

    canvas.restoreState()


def format_text(text, styles, style_name='BodyText2'):
    """Convert markdown-ish text to reportlab Paragraphs."""
    if not text:
        return []

    elements = []
    paragraphs = text.split('\n\n')

    for para in paragraphs:
        para = para.strip()
        if not para:
            continue

        # Convert inline citations [1] to colored links
        import re
        para = re.sub(r'\[(\d+)\]', r'<font color="#2b6cb0"><b>[\1]</b></font>', para)

        # Bold
        para = re.sub(r'\*\*(.+?)\*\*', r'<b>\1</b>', para)
        # Italic
        para = re.sub(r'\*(.+?)\*', r'<i>\1</i>', para)

        elements.append(Paragraph(para, styles[style_name]))

    return elements


def build_pdf(config, output_path):
    """Build the PDF document from config."""
    styles = get_styles()

    title = config.get('title', 'Analysis Report')
    date = config.get('date', datetime.now().strftime('%Y-%m-%d %H:%M'))
    authors = config.get('authors', '')

    doc = SimpleDocTemplate(
        output_path,
        pagesize=letter,
        topMargin=0.85*inch,
        bottomMargin=0.75*inch,
        leftMargin=0.75*inch,
        rightMargin=0.75*inch,
        title=title,
        author=authors or "Operon IDE",
    )

    elements = []

    # ── Title Page ──
    elements.append(Spacer(1, 1.5*inch))
    elements.append(HRFlowable(width="80%", thickness=2, color=ACCENT, spaceAfter=20))
    elements.append(Paragraph(title, styles['ReportTitle']))
    if authors:
        elements.append(Paragraph(authors, styles['ReportSubtitle']))
    elements.append(Paragraph(f"Generated: {date}", styles['ReportSubtitle']))
    elements.append(HRFlowable(width="80%", thickness=2, color=ACCENT, spaceBefore=20, spaceAfter=30))

    # Abstract on title page if short
    abstract_text = config.get('abstract_text', '')
    if abstract_text:
        elements.append(Spacer(1, 0.3*inch))
        elements.append(Paragraph("Abstract", styles['SectionHeading']))
        elements.append(HRFlowable(width="100%", thickness=0.5, color=LIGHT_GRAY, spaceAfter=8))
        for p in format_text(abstract_text, styles, 'Abstract'):
            elements.append(p)

    elements.append(PageBreak())

    # ── Table of Contents placeholder ──
    elements.append(Paragraph("Table of Contents", styles['SectionHeading']))
    elements.append(HRFlowable(width="100%", thickness=0.5, color=LIGHT_GRAY, spaceAfter=8))
    toc_items = ['Abstract', 'Introduction', 'Results', 'Discussion', 'Methods', 'References']
    if not config.get('introduction'):
        toc_items.remove('Introduction')
    for i, item in enumerate(toc_items, 1):
        elements.append(Paragraph(
            f"<font color='#2b6cb0'>{i}.</font>&nbsp;&nbsp;{item}",
            styles['BodyText2']
        ))
    elements.append(PageBreak())

    # ── Introduction ──
    intro = config.get('introduction', '')
    if intro:
        elements.append(Paragraph("Introduction", styles['SectionHeading']))
        elements.append(HRFlowable(width="100%", thickness=0.5, color=LIGHT_GRAY, spaceAfter=8))
        elements.extend(format_text(intro, styles))

    # ── Results ──
    results = config.get('results', '')
    if results:
        elements.append(Paragraph("Results", styles['SectionHeading']))
        elements.append(HRFlowable(width="100%", thickness=0.5, color=LIGHT_GRAY, spaceAfter=8))
        elements.extend(format_text(results, styles))

    # ── Figures ──
    figures = config.get('figures', [])
    for fig in figures:
        fig_path = fig.get('path', '')
        caption = fig.get('caption', '')
        label = fig.get('label', '')

        if os.path.exists(fig_path):
            try:
                img = Image(fig_path)
                # Scale to fit within page width
                max_width = 5.5 * inch
                max_height = 4 * inch
                iw, ih = img.drawWidth, img.drawHeight
                if iw > max_width:
                    ratio = max_width / iw
                    iw, ih = iw * ratio, ih * ratio
                if ih > max_height:
                    ratio = max_height / ih
                    iw, ih = iw * ratio, ih * ratio
                img.drawWidth = iw
                img.drawHeight = ih
                img.hAlign = 'CENTER'

                fig_elements = [
                    Spacer(1, 8),
                    img,
                    Paragraph(
                        f"<b>{label}</b> {caption}" if label else caption,
                        styles['Caption']
                    ),
                ]
                elements.append(KeepTogether(fig_elements))
            except Exception as e:
                elements.append(Paragraph(
                    f"<i>[Figure not available: {fig_path} — {e}]</i>",
                    styles['Caption']
                ))

    # ── Tables ──
    tables = config.get('tables', [])
    for tbl in tables:
        tbl_title = tbl.get('title', '')
        headers = tbl.get('headers', [])
        rows = tbl.get('rows', [])
        caption = tbl.get('caption', '')

        if headers:
            elements.append(Spacer(1, 8))
            if tbl_title:
                elements.append(Paragraph(f"<b>{tbl_title}</b>", styles['SubHeading']))

            # Build table data
            table_data = [headers] + rows[:30]  # Limit to 30 rows

            # Calculate column widths
            available_width = 6.5 * inch
            n_cols = len(headers)
            col_width = available_width / max(n_cols, 1)

            # Wrap long cell text
            wrapped_data = []
            for row_idx, row in enumerate(table_data):
                wrapped_row = []
                for cell in row:
                    cell_str = str(cell) if cell else ""
                    if len(cell_str) > 40:
                        cell_str = cell_str[:37] + "..."
                    style = styles['ToolTable'] if row_idx == 0 else styles['BodyText2']
                    wrapped_row.append(Paragraph(cell_str, style))
                wrapped_data.append(wrapped_row)

            t = Table(wrapped_data, colWidths=[col_width]*n_cols)
            t.setStyle(TableStyle([
                ('BACKGROUND', (0, 0), (-1, 0), TABLE_HEADER_BG),
                ('TEXTCOLOR', (0, 0), (-1, 0), colors.white),
                ('FONTNAME', (0, 0), (-1, 0), 'Helvetica-Bold'),
                ('FONTSIZE', (0, 0), (-1, 0), 9),
                ('FONTSIZE', (0, 1), (-1, -1), 8.5),
                ('ALIGN', (0, 0), (-1, -1), 'LEFT'),
                ('VALIGN', (0, 0), (-1, -1), 'TOP'),
                ('GRID', (0, 0), (-1, -1), 0.5, colors.Color(0.85, 0.85, 0.85)),
                ('ROWBACKGROUNDS', (0, 1), (-1, -1), [colors.white, TABLE_ALT_ROW]),
                ('TOPPADDING', (0, 0), (-1, -1), 4),
                ('BOTTOMPADDING', (0, 0), (-1, -1), 4),
                ('LEFTPADDING', (0, 0), (-1, -1), 6),
                ('RIGHTPADDING', (0, 0), (-1, -1), 6),
            ]))
            elements.append(t)

            if caption:
                elements.append(Paragraph(caption, styles['Caption']))

    # ── Discussion ──
    discussion = config.get('discussion', '')
    if discussion:
        elements.append(Paragraph("Discussion", styles['SectionHeading']))
        elements.append(HRFlowable(width="100%", thickness=0.5, color=LIGHT_GRAY, spaceAfter=8))
        elements.extend(format_text(discussion, styles))

    # ── Methods ──
    methods = config.get('methods', {})
    if methods:
        elements.append(Paragraph("Methods", styles['SectionHeading']))
        elements.append(HRFlowable(width="100%", thickness=0.5, color=LIGHT_GRAY, spaceAfter=8))

        overview = methods.get('overview', '')
        if overview:
            elements.extend(format_text(overview, styles))

        # Tools table
        method_tools = methods.get('tools', [])
        if method_tools:
            elements.append(Paragraph("Software and Tools", styles['SubHeading']))

            tool_headers = ['Tool/Package', 'Version', 'Language', 'Category']
            tool_rows = []
            for tool in method_tools:
                tool_rows.append([
                    tool.get('name', ''),
                    tool.get('version', '-'),
                    tool.get('language', '-'),
                    tool.get('category', '-'),
                ])

            tool_data = [tool_headers] + tool_rows
            col_widths = [2.2*inch, 1.2*inch, 1.2*inch, 1.9*inch]

            # Wrap in Paragraphs
            wrapped_tool_data = []
            for i, row in enumerate(tool_data):
                wrapped = []
                for cell in row:
                    s = styles['ToolTable'] if i == 0 else styles['BodyText2']
                    wrapped.append(Paragraph(str(cell), s))
                wrapped_tool_data.append(wrapped)

            tt = Table(wrapped_tool_data, colWidths=col_widths)
            tt.setStyle(TableStyle([
                ('BACKGROUND', (0, 0), (-1, 0), TABLE_HEADER_BG),
                ('TEXTCOLOR', (0, 0), (-1, 0), colors.white),
                ('FONTNAME', (0, 0), (-1, 0), 'Helvetica-Bold'),
                ('FONTSIZE', (0, 0), (-1, -1), 9),
                ('ALIGN', (0, 0), (-1, -1), 'LEFT'),
                ('VALIGN', (0, 0), (-1, -1), 'TOP'),
                ('GRID', (0, 0), (-1, -1), 0.5, colors.Color(0.85, 0.85, 0.85)),
                ('ROWBACKGROUNDS', (0, 1), (-1, -1), [colors.white, TABLE_ALT_ROW]),
                ('TOPPADDING', (0, 0), (-1, -1), 4),
                ('BOTTOMPADDING', (0, 0), (-1, -1), 4),
                ('LEFTPADDING', (0, 0), (-1, -1), 6),
            ]))
            elements.append(tt)
            elements.append(Spacer(1, 8))

        data_sources = methods.get('data_sources', '')
        if data_sources:
            elements.append(Paragraph("Data Sources", styles['SubHeading']))
            elements.extend(format_text(data_sources, styles))

    # ── References ──
    references = config.get('references', [])
    if references:
        elements.append(Paragraph("References", styles['SectionHeading']))
        elements.append(HRFlowable(width="100%", thickness=0.5, color=LIGHT_GRAY, spaceAfter=8))

        for ref in references:
            idx = ref.get('index', 0)
            ref_authors = ref.get('authors', '')
            ref_title = ref.get('title', '')
            ref_journal = ref.get('journal', '')
            ref_year = ref.get('year', '')
            ref_pmid = ref.get('pmid', '')
            ref_doi = ref.get('doi', '')
            ref_url = ref.get('url', '')

            # Format: [1] Authors. "Title". Journal (Year). PMID: xxx
            ref_text = (
                f'<font color="#2b6cb0"><b>[{idx}]</b></font> '
                f'{ref_authors}. '
                f'<i>"{ref_title}"</i>. '
                f'{ref_journal} ({ref_year}). '
            )
            if ref_pmid:
                ref_text += f'PMID: <font color="#2b6cb0"><link href="{ref_url}">{ref_pmid}</link></font>'
            if ref_doi:
                ref_text += f' | DOI: {ref_doi}'

            elements.append(Paragraph(ref_text, styles['Reference']))

    # ── Build ──
    def on_page(canvas, doc):
        add_header_footer(canvas, doc, title=title, date=date)

    doc.build(elements, onFirstPage=on_page, onLaterPages=on_page)
    print(f"Report generated: {output_path}")


def main():
    if len(sys.argv) != 3:
        print(f"Usage: {sys.argv[0]} <config.json> <output.pdf>", file=sys.stderr)
        sys.exit(1)

    config_path = sys.argv[1]
    output_path = sys.argv[2]

    with open(config_path, 'r') as f:
        config = json.load(f)

    # Ensure output directory exists
    output_dir = os.path.dirname(output_path)
    if output_dir:
        os.makedirs(output_dir, exist_ok=True)

    build_pdf(config, output_path)


if __name__ == '__main__':
    main()
