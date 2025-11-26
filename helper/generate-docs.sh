#!/bin/bash
# Generate HTML documentation from markdown sources
# Usage: ./helper/generate-docs.sh

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
DOCS_DIR="$PROJECT_ROOT/docs"

echo "Generating HTML documentation..."

# CSS for light mode
read -r -d '' LIGHT_CSS << 'EOFCSS' || true
<style>
:root {
    --bg-color: #ffffff;
    --text-color: #333333;
    --heading-color: #1a1a1a;
    --link-color: #0066cc;
    --code-bg: #f4f4f4;
    --border-color: #dddddd;
    --table-stripe: #f9f9f9;
}
body {
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif;
    line-height: 1.6;
    max-width: 900px;
    margin: 0 auto;
    padding: 20px;
    background-color: var(--bg-color);
    color: var(--text-color);
}
h1, h2, h3, h4 { color: var(--heading-color); margin-top: 1.5em; }
h1 { border-bottom: 2px solid var(--border-color); padding-bottom: 0.3em; }
h2 { border-bottom: 1px solid var(--border-color); padding-bottom: 0.2em; }
a { color: var(--link-color); text-decoration: none; }
a:hover { text-decoration: underline; }
code {
    background-color: var(--code-bg);
    padding: 0.2em 0.4em;
    border-radius: 3px;
    font-family: "SFMono-Regular", Consolas, "Liberation Mono", Menlo, monospace;
    font-size: 0.9em;
}
pre {
    background-color: var(--code-bg);
    padding: 1em;
    border-radius: 5px;
    overflow-x: auto;
    border: 1px solid var(--border-color);
}
pre code { background: none; padding: 0; }
table {
    border-collapse: collapse;
    width: 100%;
    margin: 1em 0;
}
th, td {
    border: 1px solid var(--border-color);
    padding: 0.5em 1em;
    text-align: left;
}
th { background-color: var(--code-bg); font-weight: 600; }
tr:nth-child(even) { background-color: var(--table-stripe); }
blockquote {
    border-left: 4px solid var(--link-color);
    margin: 1em 0;
    padding-left: 1em;
    color: #666;
}
hr { border: none; border-top: 1px solid var(--border-color); margin: 2em 0; }
ul, ol { padding-left: 2em; }
li { margin: 0.3em 0; }
.mode-switcher {
    position: fixed;
    top: 10px;
    right: 10px;
    padding: 8px 16px;
    background: var(--code-bg);
    border: 1px solid var(--border-color);
    border-radius: 5px;
    cursor: pointer;
    font-size: 14px;
    text-decoration: none !important;
}
.mode-switcher:hover { background: var(--border-color); }
</style>
EOFCSS

# CSS for dark mode
read -r -d '' DARK_CSS << 'EOFCSS' || true
<style>
:root {
    --bg-color: #1a1a2e;
    --text-color: #e0e0e0;
    --heading-color: #ffffff;
    --link-color: #6db3f2;
    --code-bg: #16213e;
    --border-color: #404040;
    --table-stripe: #1f1f3a;
}
body {
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif;
    line-height: 1.6;
    max-width: 900px;
    margin: 0 auto;
    padding: 20px;
    background-color: var(--bg-color);
    color: var(--text-color);
}
h1, h2, h3, h4 { color: var(--heading-color); margin-top: 1.5em; }
h1 { border-bottom: 2px solid var(--border-color); padding-bottom: 0.3em; }
h2 { border-bottom: 1px solid var(--border-color); padding-bottom: 0.2em; }
a { color: var(--link-color); text-decoration: none; }
a:hover { text-decoration: underline; }
code {
    background-color: var(--code-bg);
    padding: 0.2em 0.4em;
    border-radius: 3px;
    font-family: "SFMono-Regular", Consolas, "Liberation Mono", Menlo, monospace;
    font-size: 0.9em;
}
pre {
    background-color: var(--code-bg);
    padding: 1em;
    border-radius: 5px;
    overflow-x: auto;
    border: 1px solid var(--border-color);
}
pre code { background: none; padding: 0; }
table {
    border-collapse: collapse;
    width: 100%;
    margin: 1em 0;
}
th, td {
    border: 1px solid var(--border-color);
    padding: 0.5em 1em;
    text-align: left;
}
th { background-color: var(--code-bg); font-weight: 600; }
tr:nth-child(even) { background-color: var(--table-stripe); }
blockquote {
    border-left: 4px solid var(--link-color);
    margin: 1em 0;
    padding-left: 1em;
    color: #999;
}
hr { border: none; border-top: 1px solid var(--border-color); margin: 2em 0; }
ul, ol { padding-left: 2em; }
li { margin: 0.3em 0; }
.mode-switcher {
    position: fixed;
    top: 10px;
    right: 10px;
    padding: 8px 16px;
    background: var(--code-bg);
    border: 1px solid var(--border-color);
    border-radius: 5px;
    cursor: pointer;
    font-size: 14px;
    color: var(--text-color);
    text-decoration: none !important;
}
.mode-switcher:hover { background: var(--border-color); }
</style>
EOFCSS

# Python script for markdown conversion (more reliable than sed)
read -r -d '' PYTHON_CONVERTER << 'EOFPY' || true
import sys
import re
import html

def convert_md_to_html(md_text):
    lines = md_text.split('\n')
    html_lines = []
    in_code_block = False
    in_table = False
    in_list = False
    list_type = None

    i = 0
    while i < len(lines):
        line = lines[i]

        # Code blocks
        if line.startswith('```'):
            if in_code_block:
                html_lines.append('</code></pre>')
                in_code_block = False
            else:
                lang = line[3:].strip()
                html_lines.append(f'<pre><code class="language-{lang}">' if lang else '<pre><code>')
                in_code_block = True
            i += 1
            continue

        if in_code_block:
            html_lines.append(html.escape(line))
            i += 1
            continue

        # Empty line
        if not line.strip():
            if in_list:
                html_lines.append(f'</{list_type}>')
                in_list = False
                list_type = None
            if in_table:
                html_lines.append('</table>')
                in_table = False
            html_lines.append('')
            i += 1
            continue

        # Tables
        if '|' in line and line.strip().startswith('|'):
            cells = [c.strip() for c in line.strip().strip('|').split('|')]
            if not in_table:
                html_lines.append('<table>')
                in_table = True
                # Check if next line is separator
                if i + 1 < len(lines) and re.match(r'^\|[-:|]+\|$', lines[i+1].strip()):
                    html_lines.append('<tr>' + ''.join(f'<th>{html.escape(c)}</th>' for c in cells) + '</tr>')
                    i += 2  # Skip separator line
                    continue
            html_lines.append('<tr>' + ''.join(f'<td>{process_inline(c)}</td>' for c in cells) + '</tr>')
            i += 1
            continue

        if in_table:
            html_lines.append('</table>')
            in_table = False

        # Headers
        header_match = re.match(r'^(#{1,6})\s+(.+)$', line)
        if header_match:
            level = len(header_match.group(1))
            text = process_inline(header_match.group(2))
            # Generate ID for anchor links
            anchor_id = re.sub(r'[^a-z0-9-]', '', header_match.group(2).lower().replace(' ', '-'))
            html_lines.append(f'<h{level} id="{anchor_id}">{text}</h{level}>')
            i += 1
            continue

        # Horizontal rule
        if re.match(r'^-{3,}$', line.strip()):
            html_lines.append('<hr>')
            i += 1
            continue

        # Unordered list
        list_match = re.match(r'^(\s*)[-*]\s+(.+)$', line)
        if list_match:
            if not in_list or list_type != 'ul':
                if in_list:
                    html_lines.append(f'</{list_type}>')
                html_lines.append('<ul>')
                in_list = True
                list_type = 'ul'
            html_lines.append(f'<li>{process_inline(list_match.group(2))}</li>')
            i += 1
            continue

        # Ordered list
        ol_match = re.match(r'^(\s*)\d+\.\s+(.+)$', line)
        if ol_match:
            if not in_list or list_type != 'ol':
                if in_list:
                    html_lines.append(f'</{list_type}>')
                html_lines.append('<ol>')
                in_list = True
                list_type = 'ol'
            html_lines.append(f'<li>{process_inline(ol_match.group(2))}</li>')
            i += 1
            continue

        if in_list:
            html_lines.append(f'</{list_type}>')
            in_list = False
            list_type = None

        # Paragraph
        html_lines.append(f'<p>{process_inline(line)}</p>')
        i += 1

    # Close any open elements
    if in_list:
        html_lines.append(f'</{list_type}>')
    if in_table:
        html_lines.append('</table>')
    if in_code_block:
        html_lines.append('</code></pre>')

    return '\n'.join(html_lines)

def process_inline(text):
    # Escape HTML first (but preserve already processed)
    text = html.escape(text)
    # Links [text](url)
    text = re.sub(r'\[([^\]]+)\]\(([^)]+)\)', r'<a href="\2">\1</a>', text)
    # Bold **text**
    text = re.sub(r'\*\*([^*]+)\*\*', r'<strong>\1</strong>', text)
    # Italic *text*
    text = re.sub(r'\*([^*]+)\*', r'<em>\1</em>', text)
    # Inline code `text`
    text = re.sub(r'`([^`]+)`', r'<code>\1</code>', text)
    return text

if __name__ == '__main__':
    md_content = sys.stdin.read()
    print(convert_md_to_html(md_content))
EOFPY

# Function to convert markdown to HTML
convert_md_to_html() {
    local input="$1"
    local output="$2"
    local css="$3"
    local mode_link="$4"
    local mode_text="$5"

    local body_content=""

    # Check if pandoc is available (best option)
    if command -v pandoc &> /dev/null; then
        body_content=$(pandoc "$input" -f markdown -t html)
    # Check if python3 is available
    elif command -v python3 &> /dev/null; then
        # Write the Python script to a temp file to avoid shell escaping issues
        local tmp_script=$(mktemp)
        echo "$PYTHON_CONVERTER" > "$tmp_script"
        body_content=$(python3 "$tmp_script" < "$input")
        rm -f "$tmp_script"
    else
        echo "Error: Neither pandoc nor python3 found. Please install one of them." >&2
        exit 1
    fi

    # Generate the full HTML document
    cat > "$output" << EOFHTML
<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>Requirements Manager User Guide</title>
$css
</head>
<body>
<a href="$mode_link" class="mode-switcher">$mode_text</a>
$body_content
</body>
</html>
EOFHTML
}

# Generate light mode version
echo "  Generating light mode..."
convert_md_to_html \
    "$DOCS_DIR/user-guide.md" \
    "$DOCS_DIR/user-guide.html" \
    "$LIGHT_CSS" \
    "user-guide-dark.html" \
    "Switch to Dark Mode"

# Generate dark mode version
echo "  Generating dark mode..."
convert_md_to_html \
    "$DOCS_DIR/user-guide.md" \
    "$DOCS_DIR/user-guide-dark.html" \
    "$DARK_CSS" \
    "user-guide.html" \
    "Switch to Light Mode"

echo "Done! Generated:"
echo "  - $DOCS_DIR/user-guide.html (light mode)"
echo "  - $DOCS_DIR/user-guide-dark.html (dark mode)"
