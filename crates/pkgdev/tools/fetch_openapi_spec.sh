#!/usr/bin/env bash

cd .. || exit

mkdir -p spec

curl http://localhost:3100/api-docs/openapi.json -o spec/forged.spec.json