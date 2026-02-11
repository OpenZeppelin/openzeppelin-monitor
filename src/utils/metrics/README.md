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

## Available Metrics

### System Metrics

| Metric | Type | Description |
|--------|------|-------------|
| `cpu_usage_percentage` | Gauge | Current CPU usage (0-100) |
| `memory_usage_percentage` | Gauge | Memory usage percentage (0-100) |
| `memory_usage_bytes` | Gauge | Memory in use (bytes) |
| `total_memory_bytes` | Gauge | Total system memory (bytes) |
| `available_memory_bytes` | Gauge | Available memory (bytes) |
| `disk_usage_bytes` | Gauge | Used disk space (bytes) |
| `disk_usage_percentage` | Gauge | Disk usage percentage (0-100) |

### Monitoring Metrics

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `monitors_total` | Gauge | - | Total configured monitors |
| `monitors_active` | Gauge | - | Active (non-paused) monitors |
| `triggers_total` | Gauge | - | Total configured triggers |
| `contracts_monitored` | Gauge | - | Unique contracts being monitored |
| `networks_monitored` | Gauge | - | Networks with active monitors |
| `network_monitors` | Gauge | network | Monitors per network |

### RPC Metrics

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `rpc_requests_total` | Counter | network, method | Total RPC requests |
| `rpc_request_errors_total` | Counter | network, status_code, error_type | Failed RPC requests |
| `rpc_request_duration_seconds` | Histogram | network | Request latency distribution |
| `rpc_endpoint_rotations_total` | Counter | network, reason | Endpoint rotations |
| `rpc_rate_limits_total` | Counter | network, endpoint | HTTP 429 responses |

## Example Grafana Alerts

```promql
# Alert on rate limiting
rate(rpc_rate_limits_total[5m]) > 0

# Alert on high error rate
rate(rpc_request_errors_total[5m]) / rate(rpc_requests_total[5m]) > 0.1

# Alert on high latency (95th percentile > 5s)
histogram_quantile(0.95, rate(rpc_request_duration_seconds_bucket[5m])) > 5
```
