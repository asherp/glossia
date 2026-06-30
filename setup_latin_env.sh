#!/bin/bash
# Setup script for Latin wordlist generation environment

set -e

echo "Setting up conda environment for Latin wordlist generation..."
echo ""

# Check if conda is available
if ! command -v conda &> /dev/null; then
    echo "Error: conda is not installed or not in PATH"
    echo "Please install conda/miniconda/anaconda first"
    exit 1
fi

# Create the environment
echo "Creating conda environment 'cltk'..."
conda env create -f environment.yml

echo ""
echo "Environment created successfully!"
echo ""
echo "To activate the environment, run:"
echo "  conda activate cltk"
echo ""
echo "Then download CLTK models:"
echo "  python -c \"from cltk.data.fetch import FetchCorpus; FetchCorpus('lat').import_corpus('lat_models_cltk')\""
echo ""
echo "And generate your wordlist:"
echo "  python glossia-tools/py/generate_latin_wordlist.py -o latin_wordlist.yaml"
