#!/usr/bin/env python3
"""Script to read and extract content from a PDF file."""

import fitz  # PyMuPDF
import sys

def read_pdf(pdf_path):
    """Read a PDF file and extract text content."""
    try:
        # Open the PDF
        doc = fitz.open(pdf_path)
        
        print(f"PDF opened successfully!")
        print(f"Number of pages: {len(doc)}")
        print(f"Metadata: {doc.metadata}\n")
        
        # Extract text from all pages
        full_text = []
        for page_num in range(len(doc)):
            page = doc[page_num]
            text = page.get_text()
            full_text.append(f"--- Page {page_num + 1} ---\n{text}\n")
        
        doc.close()
        
        return "\n".join(full_text)
    
    except Exception as e:
        print(f"Error reading PDF: {e}", file=sys.stderr)
        return None

if __name__ == "__main__":
    pdf_path = "/Users/asherp/Documents/harrypotter.pdf"
    
    print(f"Reading PDF: {pdf_path}\n")
    text = read_pdf(pdf_path)
    
    if text:
        # Print first 2000 characters as a preview
        print("=" * 80)
        print("PREVIEW (first 2000 characters):")
        print("=" * 80)
        print(text[:2000])
        print("\n" + "=" * 80)
        print(f"Total characters extracted: {len(text)}")
        
        # Optionally save to a file
        output_file = "harrypotter_extracted.txt"
        with open(output_file, "w", encoding="utf-8") as f:
            f.write(text)
        print(f"\nFull text saved to: {output_file}")
