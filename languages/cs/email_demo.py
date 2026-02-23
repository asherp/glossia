#!/usr/bin/env python3
"""
Email dialect demo for Glossia.

Demonstrates:
1. Encoding data into RFC 5322 email structure via glossia
2. Generating a realistic email with Python's email module
3. Parsing the glossia-encoded email to extract payload
4. Round-trip verification

Usage:
    python3 languages/cs/email_demo.py
"""

import subprocess
import sys
import email
from email.mime.text import MIMEText
from email.mime.multipart import MIMEMultipart
from email.utils import formatdate
from pathlib import Path

# Find the project root (this script lives in languages/cs/)
SCRIPT_DIR = Path(__file__).resolve().parent
PROJECT_ROOT = SCRIPT_DIR.parent.parent

GLOSSIA = str(PROJECT_ROOT / "target" / "debug" / "glossia")


def run_glossia(*args, stdin_text=None):
    """Run glossia CLI and return (stdout, stderr, returncode)."""
    cmd = [GLOSSIA] + list(args)
    result = subprocess.run(
        cmd,
        input=stdin_text,
        capture_output=True,
        text=True,
        cwd=str(PROJECT_ROOT),
    )
    return result.stdout.strip(), result.stderr.strip(), result.returncode


def section(title):
    """Print a section header."""
    print(f"\n{'='*60}")
    print(f"  {title}")
    print(f"{'='*60}\n")


# ── 1. Encode data into email structure ──────────────────────────

section("1. Encode data into email structure (glossia)")

test_message = "Hello Bob, the meeting is at 3pm tomorrow."
print(f"Input: {test_message!r}\n")

for dialect, label in [
    ("email", "Simple text/plain"),
    ("email_alt", "Multipart/alternative"),
    ("email_mime", "Multipart/mixed"),
]:
    print(f"--- {label} (cs/{dialect}) ---")
    out, err, rc = run_glossia(
        "--into", f"cs/base64/{dialect}",
        "--seed", "42",
        stdin_text=test_message,
    )
    if rc != 0:
        print(f"  ERROR: {err}")
    else:
        # Indent each line for readability
        for line in out.split("\n"):
            print(f"  {line}")
    print()


# ── 2. Generate a real email with Python ──────────────────────────

section("2. Generate a realistic email with Python's email module")

# Build a multipart/alternative email (like Gmail sends)
msg = MIMEMultipart("alternative", boundary="glossia-alt")
msg["From"] = "Alice Example <alice@example.com>"
msg["To"] = "Bob Example <bob@gmail.com>"
msg["Date"] = formatdate(localtime=True)
msg["Subject"] = "Meeting reminder"
msg["Message-ID"] = "<20260222.001@mail.example.com>"
msg["MIME-Version"] = "1.0"

# Text part
text_body = """\
Hi Bob,

Just a reminder about our meeting tomorrow at 3pm.
Please bring the quarterly report.

Best,
Alice"""

html_body = """\
<html>
<body>
<p>Hi Bob,</p>
<p>Just a reminder about our meeting tomorrow at <b>3pm</b>.</p>
<p>Please bring the quarterly report.</p>
<p>Best,<br>Alice</p>
</body>
</html>"""

msg.attach(MIMEText(text_body, "plain"))
msg.attach(MIMEText(html_body, "html"))

raw_email = msg.as_string()
print("Generated email:")
for line in raw_email.split("\n")[:25]:
    print(f"  {line}")
print("  ...")
print(f"\n  (Total: {len(raw_email)} chars, {len(raw_email.split(chr(10)))} lines)")


# ── 3. Parse email structure ──────────────────────────────────────

section("3. Parse the email with Python's email module")

parsed = email.message_from_string(raw_email)
print(f"  From:    {parsed['From']}")
print(f"  To:      {parsed['To']}")
print(f"  Subject: {parsed['Subject']}")
print(f"  Date:    {parsed['Date']}")
print(f"  MIME:    {parsed.get_content_type()}")
print(f"  Parts:   {sum(1 for _ in parsed.walk()) - 1}")  # -1 for container
print()

for i, part in enumerate(parsed.walk()):
    ct = part.get_content_type()
    if ct.startswith("multipart/"):
        continue
    payload = part.get_payload(decode=True)
    if payload:
        preview = payload.decode("utf-8", errors="replace")[:80]
        print(f"  Part {i}: {ct}")
        print(f"    Preview: {preview!r}...")
        print()


# ── 4. Glossia encode → structure inspection ──────────────────────

section("4. Glossia-encoded email: structure inspection")

# Encode with a known seed for reproducibility
test_data = "attack at dawn"
out, err, rc = run_glossia(
    "--into", "cs/base64/email_alt",
    "--seed", "123",
    stdin_text=test_data,
)

if rc != 0:
    print(f"ERROR: {err}")
    sys.exit(1)

print(f"Input:  {test_data!r}")
print(f"Output ({len(out)} chars):\n")
for line in out.split("\n"):
    print(f"  {line}")

# Parse the glossia output as an email
print("\nParsing glossia output as email...")
glossia_email = email.message_from_string(out)
print(f"  From:    {glossia_email['From']}")
print(f"  To:      {glossia_email['To']}")
print(f"  Subject: {glossia_email['Subject']}")
print(f"  MIME:    {glossia_email.get_content_type()}")

# Walk parts
for i, part in enumerate(glossia_email.walk()):
    ct = part.get_content_type()
    if ct.startswith("multipart/"):
        continue
    payload_bytes = part.get_payload(decode=True)
    if payload_bytes:
        text = payload_bytes.decode("utf-8", errors="replace")
        print(f"\n  Part {i} ({ct}):")
        print(f"    Content: {text!r}")

        # Extract base64 payload chars (filter out non-base64)
        base64_chars = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/="
        extracted = "".join(c for c in text if c in base64_chars)
        if extracted:
            print(f"    Payload chars: {extracted}")


# ── 5. Classification demo ────────────────────────────────────────

section("5. Email anatomy classification")

print("Every part of an email is either COVER or PAYLOAD:\n")
print("  COVER (structural, transport layer):")
print("    - Return-Path, Received, Message-ID, DKIM-Signature")
print("    - From, To, Date (envelope headers)")
print("    - MIME-Version, Content-Type, Content-Transfer-Encoding")
print("    - MIME boundaries (--glossia, --glossia--)")
print("    - HTML body (structural duplicate of text/plain)")
print()
print("  PAYLOAD (content zones):")
print("    - Subject line value")
print("    - Text/plain body content")
print("    - Attachment body content")
print()
print("The grammar models this classification formally:")
print("  sentence -> ENVELOPE_HEADERS SUBJECT_LINE PREAMBLE BODY")
print("  ENVELOPE_HEADERS -> Aux[from] To[from-val] Conj ...")
print("  SUBJECT_LINE -> Aux[subject] SUBJECT_BODY Conj")
print("  SUBJECT_BODY -> N | N SUBJECT_BODY  (payload zone 1)")
print("  BODY -> N | N BODY                  (payload zone 2)")
print()

section("Done")
print("The email dialect models RFC 5322 / MIME structure as a")
print("context-free grammar where every token is classified as")
print("either cover (structural) or payload (content).")
print()
print("Tools like nostr-mail can use this grammar as a reference")
print("to parse/generate email structure, composing it with")
print("content encodings (Latin prose, base64, etc.) at the")
print("pipeline level.")
