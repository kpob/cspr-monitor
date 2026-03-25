---
name: docker-health-monitor
description: >
  Monitor Docker stack health after rebuild/restart. Use as a background agent while
  working on code changes. Reports when all services are healthy and pipeline is processing.
---

# Docker Stack Health Monitor

Monitor the casper-monitor Docker stack until all services are healthy and the pipeline is processing events.

## Instructions

1. Run `docker compose ps` to check current service status
2. If any services are not yet "healthy" or "running", wait 10 seconds and check again
3. Repeat until all services show healthy/running, or until 5 minutes have elapsed
4. Once services are healthy, check pipeline flow:
   - Run `docker exec casper-monitor-kafka-1 kafka-consumer-groups.sh --bootstrap-server localhost:29092 --describe --group event-router 2>/dev/null` to check consumer lag
   - If LAG is decreasing between checks, the pipeline is processing
5. Report final status:
   - Which services are healthy/unhealthy
   - Consumer group lag numbers
   - Any services that failed to start (check their logs with `docker logs <container> --tail 20`)

## Compose file detection

Check which compose file is in use:
- Dev: `docker compose ps` (default docker-compose.yml)
- Prod: `docker compose -f docker-compose.prod.yml ps`

Use whichever has running containers, or ask the user if unclear.
