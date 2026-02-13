# Glossia Documentation

This directory contains the mdbook documentation for Glossia.

## Building the Book

```bash
cd docs
mdbook build
```

The HTML output will be generated in `docs/book/`.

## Serving Locally

To serve the documentation locally with live reload:

```bash
cd docs
mdbook serve
```

Then open http://localhost:3000 in your browser.

## Structure

- `src/` - Source markdown files
- `book.toml` - mdbook configuration
- `book/` - Generated HTML output (gitignored)

## Adding Content

Edit the markdown files in `src/` and update `src/SUMMARY.md` to add new chapters or reorder content.
