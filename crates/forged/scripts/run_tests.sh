#!/usr/bin/env bash
set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${YELLOW}Starting test infrastructure...${NC}"
docker-compose -f docker-compose.test.yml up -d

echo -e "${YELLOW}Waiting for services to be healthy...${NC}"
docker-compose -f docker-compose.test.yml wait

echo -e "${YELLOW}Running database migrations...${NC}"
export TEST_DATABASE_URL="postgresql://forged:forged@localhost:5433/forged_test"
export TEST_SEAWEEDFS_URL="http://localhost:9334"
cargo run -p forged-migration

echo -e "${YELLOW}Running tests with nextest...${NC}"
if cargo nextest run --profile ci --all-features; then
    echo -e "${GREEN}✓ Tests passed${NC}"
    EXIT_CODE=0
else
    echo -e "${RED}✗ Tests failed${NC}"
    EXIT_CODE=1
fi

echo -e "${YELLOW}Cleaning up test infrastructure...${NC}"
docker-compose -f docker-compose.test.yml down -v

exit $EXIT_CODE
