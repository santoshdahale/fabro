#!/usr/bin/env bash
set -euo pipefail

openssl genpkey -algorithm Ed25519 -out arc-jwt-private.pem
openssl pkey -in arc-jwt-private.pem -pubout -out arc-jwt-public.pem

echo ""
echo "Generated:"
echo "  arc-jwt-private.pem  (private key — for arc-web / ARC_JWT_PRIVATE_KEY)"
echo "  arc-jwt-public.pem   (public key  — for arc-attractor / ARC_JWT_PUBLIC_KEY)"
echo ""
echo "Set env vars with the PEM contents (including header/footer lines):"
echo ""
echo '  export ARC_JWT_PRIVATE_KEY="$(cat arc-jwt-private.pem)"'
echo '  export ARC_JWT_PUBLIC_KEY="$(cat arc-jwt-public.pem)"'
