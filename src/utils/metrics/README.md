# Metrics

## Overview

The metrics system provides monitoring capabilities for the OpenZeppelin Monitor application through Prometheus and Grafana integration.

## Architecture

- A metrics server runs on port `8081`
- Middleware intercepts requests across all endpoints
- Metrics are exposed via the `/metrics` endpoint
- Prometheus collects and stores the metrics data
- Grafana provides visualization through customizable dashboards

## Access Points

- Prometheus UI: `http://localhost:9090`
- Grafana Dashboard: `http://localhost:3000`
- Raw Metrics: `http://localhost:8081/metrics`
