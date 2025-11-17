# API Gateway Helm Chart

This Helm chart deploys the Rust-based API Gateway on Kubernetes.

## Prerequisites

- Kubernetes 1.20+
- Helm 3.0+
- PV provisioner support in the underlying infrastructure (optional)
- Prometheus Operator (optional, for ServiceMonitor)

## Installing the Chart

```bash
# Add the repository (if published)
helm repo add api-gateway https://charts.example.com

# Update repositories
helm repo update

# Install the chart
helm install my-gateway api-gateway/api-gateway

# Install with custom values
helm install my-gateway api-gateway/api-gateway -f custom-values.yaml

# Install from local directory
helm install my-gateway ./helm/api-gateway
```

## Uninstalling the Chart

```bash
helm uninstall my-gateway
```

## Configuration

The following table lists the configurable parameters and their default values.

### Global Parameters

| Parameter | Description | Default |
|-----------|-------------|---------|
| `replicaCount` | Number of gateway replicas | `3` |
| `image.repository` | Gateway image repository | `api-gateway` |
| `image.tag` | Gateway image tag | `latest` |
| `image.pullPolicy` | Image pull policy | `IfNotPresent` |

### Service Parameters

| Parameter | Description | Default |
|-----------|-------------|---------|
| `service.type` | Kubernetes service type | `LoadBalancer` |
| `service.port` | Service port | `80` |
| `service.httpsPort` | HTTPS service port | `443` |

### Autoscaling Parameters

| Parameter | Description | Default |
|-----------|-------------|---------|
| `autoscaling.enabled` | Enable HPA | `true` |
| `autoscaling.minReplicas` | Minimum replicas | `3` |
| `autoscaling.maxReplicas` | Maximum replicas | `20` |
| `autoscaling.targetCPUUtilizationPercentage` | Target CPU utilization | `70` |
| `autoscaling.targetMemoryUtilizationPercentage` | Target memory utilization | `80` |

### Gateway Configuration

| Parameter | Description | Default |
|-----------|-------------|---------|
| `config.server.port` | Gateway port | `8080` |
| `config.server.timeout_secs` | Request timeout | `30` |
| `config.routes` | Route configurations | `[]` |
| `config.auth` | Authentication configuration | See values.yaml |
| `config.rate_limiting` | Rate limiting configuration | See values.yaml |

### TLS Parameters

| Parameter | Description | Default |
|-----------|-------------|---------|
| `tls.enabled` | Enable TLS | `false` |
| `tls.existingSecret` | Use existing TLS secret | `""` |
| `tls.cert` | TLS certificate (if not using existing secret) | `""` |
| `tls.key` | TLS private key (if not using existing secret) | `""` |

### Monitoring Parameters

| Parameter | Description | Default |
|-----------|-------------|---------|
| `monitoring.serviceMonitor.enabled` | Create Prometheus ServiceMonitor | `false` |
| `monitoring.serviceMonitor.interval` | Scrape interval | `30s` |

## Examples

### Basic Installation

```bash
helm install my-gateway ./helm/api-gateway \
  --set image.tag=v0.1.0 \
  --set service.type=LoadBalancer
```

### With Custom Routes

Create a `values.yaml`:

```yaml
config:
  routes:
    - path: "/api/users/*path"
      backend: "http://user-service:3000"
      methods: ["GET", "POST"]
      auth:
        required: true
        methods: ["jwt"]

    - path: "/api/orders/*path"
      backend: "http://order-service:3001"
      methods: ["GET", "POST"]

secrets:
  jwt_secret: "your-secure-secret-here"
```

Install:

```bash
helm install my-gateway ./helm/api-gateway -f values.yaml
```

### With TLS Enabled

```bash
# Create TLS secret first
kubectl create secret tls gateway-tls \
  --cert=path/to/cert.pem \
  --key=path/to/key.pem

# Install with TLS
helm install my-gateway ./helm/api-gateway \
  --set config.tls.enabled=true \
  --set tls.existingSecret=gateway-tls
```

### With Ingress

```yaml
ingress:
  enabled: true
  className: nginx
  annotations:
    cert-manager.io/cluster-issuer: "letsencrypt-prod"
  hosts:
    - host: api.example.com
      paths:
        - path: /
          pathType: Prefix
  tls:
    - secretName: api-tls
      hosts:
        - api.example.com
```

## Upgrading

```bash
# Upgrade with new values
helm upgrade my-gateway ./helm/api-gateway -f new-values.yaml

# Rollback to previous version
helm rollback my-gateway
```

## Monitoring

If Prometheus Operator is installed, enable the ServiceMonitor:

```yaml
monitoring:
  serviceMonitor:
    enabled: true
    interval: 30s
```

Access metrics at: `http://<gateway-service>:8080/metrics`

## Troubleshooting

### Check pod status
```bash
kubectl get pods -l app.kubernetes.io/name=api-gateway
```

### View logs
```bash
kubectl logs -l app.kubernetes.io/name=api-gateway -f
```

### Check configuration
```bash
kubectl get configmap my-gateway-config -o yaml
```

### Test service
```bash
kubectl port-forward svc/my-gateway 8080:80
curl http://localhost:8080/health
```
