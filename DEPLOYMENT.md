# Deployment Guide

This guide covers deploying the API Gateway in various environments: local development, Docker, Kubernetes, and cloud platforms.

## Table of Contents

1. [Prerequisites](#prerequisites)
2. [Local Development](#local-development)
3. [Docker Deployment](#docker-deployment)
4. [Kubernetes Deployment](#kubernetes-deployment)
5. [Helm Deployment](#helm-deployment)
6. [Cloud Platform Deployment](#cloud-platform-deployment)
7. [TLS/mTLS Setup](#tlsmtls-setup)
8. [Monitoring Setup](#monitoring-setup)
9. [Troubleshooting](#troubleshooting)

---

## Prerequisites

### Required Tools

- **Rust** 1.70+ (for building from source)
- **Docker** 20.10+ (for containerized deployment)
- **Kubernetes** 1.20+ (for Kubernetes deployment)
- **Helm** 3.0+ (for Helm deployment)
- **kubectl** (for Kubernetes management)

### Optional Tools

- **Redis** (for distributed rate limiting and API keys)
- **Prometheus** (for metrics collection)
- **Jaeger** or **Tempo** (for distributed tracing)
- **cert-manager** (for automatic TLS certificate management)

---

## Local Development

### Build from Source

```bash
# Clone repository
git clone https://github.com/therealutkarshpriyadarshi/gateway.git
cd gateway

# Build in release mode
cargo build --release

# Binary location
./target/release/gateway
```

### Run with Default Configuration

```bash
# Using cargo
cargo run --release

# Using binary directly
./target/release/gateway

# With custom config
./target/release/gateway /path/to/config.yaml
```

### Environment Variables

```bash
# Enable debug logging
export RUST_LOG=debug

# Set configuration path
export GATEWAY_CONFIG=/path/to/config.yaml

# Set secrets via environment
export GATEWAY_SECRET_JWT_SECRET="your-secret-here"

# Run gateway
cargo run --release
```

### Development with Hot Reload

```bash
# Install cargo-watch
cargo install cargo-watch

# Run with auto-reload
cargo watch -x run
```

---

## Docker Deployment

### Build Docker Image

```bash
# Build image
docker build -t api-gateway:latest .

# Build with specific tag
docker build -t api-gateway:v0.1.0 .

# Multi-platform build (ARM64 + AMD64)
docker buildx build --platform linux/amd64,linux/arm64 \
  -t api-gateway:latest .
```

### Run Container

```bash
# Basic run
docker run -p 8080:8080 api-gateway:latest

# With environment variables
docker run -p 8080:8080 \
  -e RUST_LOG=info \
  -e GATEWAY_SECRET_JWT_SECRET=your-secret \
  api-gateway:latest

# With custom config
docker run -p 8080:8080 \
  -v $(pwd)/config:/app/config \
  api-gateway:latest /app/config/gateway.yaml

# With Redis (using docker-compose)
docker-compose up -d
```

### Docker Compose Example

```yaml
version: '3.8'

services:
  gateway:
    image: api-gateway:latest
    ports:
      - "8080:8080"
      - "8443:8443"
    environment:
      RUST_LOG: "info,gateway=debug"
      GATEWAY_SECRET_JWT_SECRET: "${JWT_SECRET}"
    volumes:
      - ./config:/app/config
      - ./certs:/app/certs
    depends_on:
      - redis
    networks:
      - gateway-net

  redis:
    image: redis:7-alpine
    ports:
      - "6379:6379"
    networks:
      - gateway-net

networks:
  gateway-net:
    driver: bridge
```

### Health Checks

```bash
# Check gateway health
curl http://localhost:8080/health

# Check metrics
curl http://localhost:8080/metrics
```

---

## Kubernetes Deployment

### Using Kubernetes Manifests

```bash
# Create namespace
kubectl apply -f k8s/namespace.yaml

# Create secrets (update with real values first!)
kubectl apply -f k8s/secrets.yaml

# Apply all manifests
kubectl apply -f k8s/

# Check deployment status
kubectl get pods -n gateway
kubectl get svc -n gateway

# View logs
kubectl logs -n gateway -l app=api-gateway -f
```

### Using Kustomize

```bash
# Preview changes
kubectl kustomize k8s/

# Apply with kustomize
kubectl apply -k k8s/

# Delete resources
kubectl delete -k k8s/
```

### Verify Deployment

```bash
# Check pods
kubectl get pods -n gateway

# Check services
kubectl get svc -n gateway

# Check ingress
kubectl get ingress -n gateway

# Describe deployment
kubectl describe deployment api-gateway -n gateway

# View logs
kubectl logs -n gateway deployment/api-gateway --tail=100 -f
```

### Access Gateway

```bash
# Port forward for testing
kubectl port-forward -n gateway svc/api-gateway 8080:80

# Test health endpoint
curl http://localhost:8080/health

# If using LoadBalancer
GATEWAY_IP=$(kubectl get svc -n gateway api-gateway \
  -o jsonpath='{.status.loadBalancer.ingress[0].ip}')
curl http://$GATEWAY_IP/health
```

---

## Helm Deployment

### Install with Helm

```bash
# Install with default values
helm install my-gateway ./helm/api-gateway

# Install with custom values
helm install my-gateway ./helm/api-gateway \
  -f custom-values.yaml

# Install in specific namespace
helm install my-gateway ./helm/api-gateway \
  --namespace gateway \
  --create-namespace
```

### Custom Values Example

Create `production-values.yaml`:

```yaml
replicaCount: 5

image:
  repository: your-registry/api-gateway
  tag: "v0.1.0"
  pullPolicy: Always

service:
  type: LoadBalancer
  annotations:
    service.beta.kubernetes.io/aws-load-balancer-type: "nlb"

ingress:
  enabled: true
  className: nginx
  annotations:
    cert-manager.io/cluster-issuer: "letsencrypt-prod"
  hosts:
    - host: api.yourdomain.com
      paths:
        - path: /
          pathType: Prefix
  tls:
    - secretName: api-tls
      hosts:
        - api.yourdomain.com

autoscaling:
  enabled: true
  minReplicas: 5
  maxReplicas: 50
  targetCPUUtilizationPercentage: 70

resources:
  limits:
    cpu: 2000m
    memory: 1Gi
  requests:
    cpu: 200m
    memory: 256Mi

config:
  server:
    port: 8080
    timeout_secs: 30

  routes:
    - path: "/api/users/*path"
      backend: "http://user-service.services.svc.cluster.local:3000"
      methods: ["GET", "POST", "PUT", "DELETE"]
      auth:
        required: true
        methods: ["jwt"]

secrets:
  jwt_secret: "CHANGE_ME_IN_PRODUCTION"

monitoring:
  serviceMonitor:
    enabled: true
```

Deploy:

```bash
helm install my-gateway ./helm/api-gateway \
  -f production-values.yaml \
  --namespace gateway \
  --create-namespace
```

### Upgrade Deployment

```bash
# Upgrade with new values
helm upgrade my-gateway ./helm/api-gateway \
  -f production-values.yaml

# Upgrade and wait for completion
helm upgrade my-gateway ./helm/api-gateway \
  -f production-values.yaml \
  --wait --timeout 5m

# Rollback if needed
helm rollback my-gateway
```

### Uninstall

```bash
# Uninstall release
helm uninstall my-gateway -n gateway

# With namespace cleanup
helm uninstall my-gateway -n gateway
kubectl delete namespace gateway
```

---

## Cloud Platform Deployment

### AWS EKS

```bash
# Create EKS cluster
eksctl create cluster \
  --name gateway-cluster \
  --region us-west-2 \
  --nodegroup-name standard-workers \
  --node-type t3.medium \
  --nodes 3 \
  --nodes-min 3 \
  --nodes-max 10

# Configure kubectl
aws eks update-kubeconfig --region us-west-2 --name gateway-cluster

# Deploy with Helm
helm install my-gateway ./helm/api-gateway \
  -f aws-values.yaml \
  --namespace gateway \
  --create-namespace

# Create ingress with ALB
# (Requires AWS Load Balancer Controller)
```

### Google GKE

```bash
# Create GKE cluster
gcloud container clusters create gateway-cluster \
  --zone us-central1-a \
  --num-nodes 3 \
  --machine-type n1-standard-2 \
  --enable-autoscaling \
  --min-nodes 3 \
  --max-nodes 10

# Get credentials
gcloud container clusters get-credentials gateway-cluster \
  --zone us-central1-a

# Deploy
helm install my-gateway ./helm/api-gateway \
  -f gcp-values.yaml \
  --namespace gateway \
  --create-namespace
```

### Azure AKS

```bash
# Create resource group
az group create --name gateway-rg --location eastus

# Create AKS cluster
az aks create \
  --resource-group gateway-rg \
  --name gateway-cluster \
  --node-count 3 \
  --enable-addons monitoring \
  --generate-ssh-keys

# Get credentials
az aks get-credentials \
  --resource-group gateway-rg \
  --name gateway-cluster

# Deploy
helm install my-gateway ./helm/api-gateway \
  -f azure-values.yaml \
  --namespace gateway \
  --create-namespace
```

---

## TLS/mTLS Setup

### Generate Self-Signed Certificates (Development)

```bash
# Generate private key
openssl genrsa -out tls.key 2048

# Generate certificate
openssl req -new -x509 -key tls.key -out tls.crt -days 365 \
  -subj "/CN=api.example.com"

# For mTLS: Generate CA
openssl genrsa -out ca.key 2048
openssl req -new -x509 -key ca.key -out ca.crt -days 365 \
  -subj "/CN=Gateway CA"
```

### Create Kubernetes TLS Secret

```bash
# Create secret from files
kubectl create secret tls gateway-tls \
  --cert=tls.crt \
  --key=tls.key \
  --namespace gateway

# For mTLS: Add CA certificate
kubectl create secret generic gateway-ca \
  --from-file=ca.crt=ca.crt \
  --namespace gateway
```

### Configure Gateway for TLS

```yaml
# In Helm values
tls:
  enabled: true
  existingSecret: "gateway-tls"

config:
  tls:
    enabled: true
    cert_path: "/etc/gateway/tls/tls.crt"
    key_path: "/etc/gateway/tls/tls.key"
    enable_mtls: true
    ca_cert_path: "/etc/gateway/tls/ca.crt"
    require_client_cert: true
```

### Using cert-manager (Production)

```bash
# Install cert-manager
kubectl apply -f https://github.com/cert-manager/cert-manager/releases/download/v1.13.0/cert-manager.yaml

# Create ClusterIssuer
cat <<EOF | kubectl apply -f -
apiVersion: cert-manager.io/v1
kind: ClusterIssuer
metadata:
  name: letsencrypt-prod
spec:
  acme:
    server: https://acme-v02.api.letsencrypt.org/directory
    email: admin@example.com
    privateKeySecretRef:
      name: letsencrypt-prod
    solvers:
    - http01:
        ingress:
          class: nginx
EOF

# Ingress will automatically request certificate
```

---

## Monitoring Setup

### Prometheus Metrics

```bash
# If using Prometheus Operator, enable ServiceMonitor
helm upgrade my-gateway ./helm/api-gateway \
  --set monitoring.serviceMonitor.enabled=true \
  --reuse-values

# Access metrics
kubectl port-forward -n gateway svc/api-gateway 8080:80
curl http://localhost:8080/metrics
```

### Grafana Dashboard

```bash
# Import pre-built dashboard (example)
# Dashboard ID: TBD

# Or create custom dashboard with queries:
# - rate(http_requests_total[5m])
# - histogram_quantile(0.95, http_request_duration_seconds_bucket)
```

### Distributed Tracing

```bash
# Deploy Jaeger
kubectl create namespace observability
kubectl apply -n observability -f https://github.com/jaegertracing/jaeger-operator/releases/download/v1.49.0/jaeger-operator.yaml

# Configure gateway
helm upgrade my-gateway ./helm/api-gateway \
  --set config.observability.tracing.enabled=true \
  --set config.observability.tracing.otlp_endpoint=http://jaeger-collector.observability:4317 \
  --reuse-values
```

---

## Troubleshooting

### Pod Not Starting

```bash
# Check pod status
kubectl get pods -n gateway

# Describe pod
kubectl describe pod <pod-name> -n gateway

# View logs
kubectl logs <pod-name> -n gateway

# Common issues:
# - ImagePullBackOff: Check image name/tag
# - CrashLoopBackOff: Check logs for errors
# - Pending: Check resources/node capacity
```

### Service Not Accessible

```bash
# Check service
kubectl get svc -n gateway
kubectl describe svc api-gateway -n gateway

# Test from within cluster
kubectl run -it --rm debug --image=curlimages/curl --restart=Never -- \
  curl http://api-gateway.gateway.svc.cluster.local/health

# Check endpoints
kubectl get endpoints api-gateway -n gateway
```

### High Memory/CPU Usage

```bash
# Check resource usage
kubectl top pods -n gateway

# Scale up if needed
kubectl scale deployment api-gateway -n gateway --replicas=10

# Or update HPA
kubectl edit hpa api-gateway-hpa -n gateway
```

### Configuration Issues

```bash
# View current config
kubectl get configmap gateway-config -n gateway -o yaml

# Update config
kubectl edit configmap gateway-config -n gateway

# Restart pods to pick up changes
kubectl rollout restart deployment api-gateway -n gateway
```

### TLS Issues

```bash
# Check secret exists
kubectl get secret gateway-tls -n gateway

# Describe certificate (if using cert-manager)
kubectl describe certificate gateway-tls -n gateway

# Check certificate details
kubectl get secret gateway-tls -n gateway -o jsonpath='{.data.tls\.crt}' | base64 -d | openssl x509 -text -noout
```

---

## Performance Tuning

### Resource Allocation

```yaml
resources:
  requests:
    cpu: 500m
    memory: 512Mi
  limits:
    cpu: 2000m
    memory: 2Gi
```

### Horizontal Pod Autoscaling

```yaml
autoscaling:
  enabled: true
  minReplicas: 5
  maxReplicas: 100
  targetCPUUtilizationPercentage: 60
  targetMemoryUtilizationPercentage: 70
```

### Connection Pooling

Gateway uses reqwest with connection pooling by default. Tune if needed:

```rust
// In proxy module
let client = reqwest::Client::builder()
    .pool_max_idle_per_host(100)
    .build()?;
```

---

## Security Best Practices

1. **Always use TLS in production**
2. **Rotate secrets regularly**
3. **Use RBAC to limit access**
4. **Enable network policies**
5. **Scan images for vulnerabilities**
6. **Use pod security policies/standards**
7. **Implement rate limiting**
8. **Monitor for anomalies**

---

## Next Steps

- Review [OPERATIONS.md](OPERATIONS.md) for day-to-day operations
- Review [SECURITY.md](SECURITY.md) for security hardening
- Set up monitoring and alerting
- Configure backups for configuration
- Plan disaster recovery procedures

---

For questions or issues, please open an issue on [GitHub](https://github.com/therealutkarshpriyadarshi/gateway/issues).
