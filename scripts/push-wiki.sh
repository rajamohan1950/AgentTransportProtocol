#!/bin/bash
# Push all wiki pages to GitHub
# Run AFTER initializing the wiki at:
# https://github.com/rajamohan1950/AgentTransportProtocol/wiki
#
# Steps:
#   1. Go to the URL above
#   2. Click "Create the first page"
#   3. Save it (any content, it will be overwritten)
#   4. Run this script: bash scripts/push-wiki.sh

set -e

WIKI_DIR="/tmp/AgentTransportProtocol.wiki"

echo "Cloning wiki repo..."
rm -rf "$WIKI_DIR"
git clone https://github.com/rajamohan1950/AgentTransportProtocol.wiki.git "$WIKI_DIR"

echo "Copying wiki pages..."
cp /tmp/AgentTransportProtocol.wiki.source/*.md "$WIKI_DIR/" 2>/dev/null || true

cd "$WIKI_DIR"
git add -A
git commit -m "Add comprehensive ATP wiki — 17 pages" || echo "No changes"
git push origin master 2>/dev/null || git push origin main

echo ""
echo "✅ Wiki published!"
echo "👉 https://github.com/rajamohan1950/AgentTransportProtocol/wiki"
