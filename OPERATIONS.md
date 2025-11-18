# Operations Runbook

This runbook provides operational procedures for running and maintaining the API Gateway in production.

## Table of Contents

1. [Daily Operations](#daily-operations)
2. [Monitoring & Alerting](#monitoring--alerting)
3. [Incident Response](#incident-response)
4. [Scaling Operations](#scaling-operations)
5. [Configuration Management](#configuration-management)
6. [Backup & Recovery](#backup--recovery)
7. [Security Operations](#security-operations)
8. [Performance Optimization](#performance-optimization)
9. [Troubleshooting Guide](#troubleshooting-guide)
10. [Maintenance Windows](#maintenance-windows)

---

## Daily Operations

### Health Checks

**Morning Checklist:**

```bash
# 1. Check pod health
kubectl get pods -n gateway
# All pods should be "Running" with 1/1 ready

# 2. Check service endpoints
kubectl get endpoints api-gateway -n gateway
# Should have multiple IP addresses listed

# 3. Test health endpoint
curl https://api.yourdomain.com/health
# Should return 200 OK

# 4. Check metrics endpoint
curl https://api.yourdomain.com/metrics | head -20
# Should return Prometheus metrics

# 5. Review error rates (last hour)
kubectl logs -n gateway -l app=api-gateway --since=1h | grep ERROR
```

### Log Review

```bash
# View recent logs
kubectl logs -n gateway -l app=api-gateway --tail=100

# Follow logs in real-time
kubectl logs -n gateway -l app=api-gateway -f

# Search for errors
kubectl logs -n gateway -l app=api-gateway --since=24h | grep -i error

# Export logs for analysis
kubectl logs -n gateway -l app=api-gateway --since=24h > gateway-logs-$(date +%Y%m%d).log
```

### Metrics Dashboard Review

**Key Metrics to Monitor:**

1. **Request Rate**
   - Total requests per second
   - Requests by route
   - Requests by status code

2. **Latency**
   - p50, p95, p99 latencies
   - Should be: p50 < 5ms, p95 < 15ms, p99 < 50ms

3. **Error Rate**
   - HTTP 4xx errors (client errors)
   - HTTP 5xx errors (server errors)
   - Target: < 0.1% error rate

4. **Resource Usage**
   - CPU utilization (target < 70%)
   - Memory utilization (target < 80%)
   - Network bandwidth

5. **Backend Health**
   - Circuit breaker status
   - Backend response times
   - Backend error rates

---

## Monitoring & Alerting

### Prometheus Queries

**Request Rate:**
```promql
# Total request rate
rate(http_requests_total[5m])

# By status code
rate(http_requests_total{status=~"5.."}[5m])
```

**Latency:**
```promql
# p95 latency
histogram_quantile(0.95, rate(http_request_duration_seconds_bucket[5m]))

# p99 latency
histogram_quantile(0.99, rate(http_request_duration_seconds_bucket[5m]))
```

**Error Rate:**
```promql
# Overall error rate
rate(http_requests_total{status=~"5.."}[5m]) / rate(http_requests_total[5m])
```

**Circuit Breaker:**
```promql
# Circuit breaker state (0=closed, 1=half_open, 2=open)
circuit_breaker_state

# Rejected requests
rate(circuit_breaker_rejected_total[5m])
```

### Alert Rules

**Critical Alerts:**

```yaml
# alerts.yaml
groups:
- name: gateway_critical
  interval: 30s
  rules:
  - alert: GatewayDown
    expr: up{job="api-gateway"} == 0
    for: 1m
    labels:
      severity: critical
    annotations:
      summary: "API Gateway is down"
      description: "Gateway {{ $labels.instance }} is down for more than 1 minute"

  - alert: HighErrorRate
    expr: rate(http_requests_total{status=~"5.."}[5m]) / rate(http_requests_total[5m]) > 0.05
    for: 5m
    labels:
      severity: critical
    annotations:
      summary: "High error rate detected"
      description: "Error rate is {{ $value | humanizePercentage }}"

  - alert: HighLatency
    expr: histogram_quantile(0.95, rate(http_request_duration_seconds_bucket[5m])) > 0.05
    for: 5m
    labels:
      severity: warning
    annotations:
      summary: "High latency detected"
      description: "p95 latency is {{ $value }}s"

  - alert: PodCrashLooping
    expr: rate(kube_pod_container_status_restarts_total{namespace="gateway"}[5m]) > 0
    for: 5m
    labels:
      severity: critical
    annotations:
      summary: "Pod is crash looping"
      description: "Pod {{ $labels.pod }} is restarting frequently"
```

### On-Call Procedures

**When Alert Fires:**

1. **Acknowledge** the alert
2. **Assess** the impact (traffic, error rate, affected routes)
3. **Investigate** using runbook procedures below
4. **Mitigate** if possible (scale, rollback, etc.)
5. **Document** actions taken
6. **Follow up** with root cause analysis

---

## Incident Response

### Incident Severity Levels

| Severity | Description | Response Time | Examples |
|----------|-------------|---------------|----------|
| **P0 - Critical** | Complete outage | < 5 minutes | Gateway down, all requests failing |
| **P1 - High** | Severe degradation | < 15 minutes | High error rate (>10%), high latency (>100ms) |
| **P2 - Medium** | Partial degradation | < 1 hour | Single backend failing, increased error rate (<5%) |
| **P3 - Low** | Minor issue | < 4 hours | Warnings in logs, minor performance degradation |

### P0: Complete Outage

**Symptoms:**
- All health checks failing
- No pods running
- No response from gateway

**Response:**

```bash
# 1. Check cluster connectivity
kubectl cluster-info

# 2. Check gateway namespace
kubectl get all -n gateway

# 3. Check pod status
kubectl get pods -n gateway -o wide

# 4. Check events
kubectl get events -n gateway --sort-by='.lastTimestamp'

# 5. If deployment missing/deleted
kubectl apply -f k8s/
# OR
helm upgrade my-gateway ./helm/api-gateway --reuse-values

# 6. If image pull issues
kubectl describe pod <pod-name> -n gateway
# Update image tag or registry credentials

# 7. If resource exhaustion
kubectl describe nodes
# Scale nodes or pods

# 8. Emergency rollback if needed
helm rollback my-gateway

# 9. Monitor recovery
watch kubectl get pods -n gateway
```

### P1: High Error Rate

**Symptoms:**
- Error rate > 10%
- Circuit breakers opening
- Timeouts increasing

**Response:**

```bash
# 1. Check error distribution
kubectl logs -n gateway -l app=api-gateway --since=5m | grep -i error | head -50

# 2. Check which backends are failing
curl https://api.yourdomain.com/metrics | grep circuit_breaker_state

# 3. Check backend health
# (depends on your backend services)

# 4. Scale up if overloaded
kubectl scale deployment api-gateway -n gateway --replicas=10

# 5. Check rate limit settings
kubectl get configmap gateway-config -n gateway -o yaml | grep -A 10 rate_limiting

# 6. If specific route failing, disable or reroute
kubectl edit configmap gateway-config -n gateway
# Comment out problematic route
kubectl rollout restart deployment api-gateway -n gateway
```

### P2: Partial Degradation

**Symptoms:**
- Single backend failing
- Elevated but acceptable error rate (1-5%)
- Some routes slow

**Response:**

```bash
# 1. Identify affected route/backend
kubectl logs -n gateway -l app=api-gateway --since=10m | grep -A 5 "502\|503\|504"

# 2. Check circuit breaker status
curl https://api.yourdomain.com/metrics | grep -E "circuit_breaker_(state|rejected)"

# 3. Verify backend connectivity
kubectl run -it --rm debug --image=curlimages/curl --restart=Never -- \
  curl http://backend-service.namespace.svc.cluster.local

# 4. If backend down, circuit breaker should handle
# Monitor for automatic recovery

# 5. If persistent, consider manual intervention on backend
# or temporarily route around it
```

---

## Scaling Operations

### Manual Scaling

**Scale Deployment:**

```bash
# Scale to specific replica count
kubectl scale deployment api-gateway -n gateway --replicas=10

# Verify scaling
kubectl get pods -n gateway -w
```

**Scale Nodes (if needed):**

```bash
# AWS EKS
eksctl scale nodegroup --cluster=gateway-cluster \
  --name=standard-workers --nodes=5

# GKE
gcloud container clusters resize gateway-cluster \
  --num-nodes=5 --zone=us-central1-a

# AKS
az aks scale --resource-group gateway-rg \
  --name gateway-cluster --node-count 5
```

### Autoscaling Configuration

**Horizontal Pod Autoscaler (HPA):**

```bash
# Check current HPA status
kubectl get hpa -n gateway

# Describe HPA
kubectl describe hpa api-gateway-hpa -n gateway

# Modify HPA
kubectl edit hpa api-gateway-hpa -n gateway
```

**Example HPA adjustments:**

```yaml
spec:
  minReplicas: 10        # Increase minimum
  maxReplicas: 100       # Increase maximum
  metrics:
  - type: Resource
    resource:
      name: cpu
      target:
        type: Utilization
        averageUtilization: 60  # Lower threshold for more aggressive scaling
```

### Cluster Autoscaler

Ensure cluster autoscaler is configured for automatic node scaling:

```bash
# Verify cluster autoscaler
kubectl get deployment cluster-autoscaler -n kube-system

# Check logs
kubectl logs -n kube-system deployment/cluster-autoscaler
```

---

## Configuration Management

### Updating Configuration

**Via Helm:**

```bash
# 1. Update values file
vim production-values.yaml

# 2. Preview changes
helm diff upgrade my-gateway ./helm/api-gateway -f production-values.yaml

# 3. Apply update
helm upgrade my-gateway ./helm/api-gateway -f production-values.yaml

# 4. Monitor rollout
kubectl rollout status deployment api-gateway -n gateway
```

**Via kubectl:**

```bash
# 1. Edit ConfigMap
kubectl edit configmap gateway-config -n gateway

# 2. Trigger pod restart to pick up changes
kubectl rollout restart deployment api-gateway -n gateway

# 3. Monitor rollout
kubectl rollout status deployment api-gateway -n gateway
```

### Rolling Back Configuration

```bash
# Via Helm
helm rollback my-gateway
helm rollback my-gateway <revision-number>

# Via kubectl
kubectl rollout undo deployment api-gateway -n gateway
kubectl rollout undo deployment api-gateway -n gateway --to-revision=2
```

### Configuration Validation

Before applying configuration changes:

```bash
# 1. Validate YAML syntax
yamllint config/gateway.yaml

# 2. Test in development first
kubectl apply -f config/gateway.yaml --dry-run=client

# 3. Apply to dev/staging before production

# 4. Monitor metrics after deployment
```

---

## Backup & Recovery

### Backup Procedures

**Configuration Backup:**

```bash
# Backup all gateway resources
kubectl get all,configmap,secret,ingress,hpa -n gateway -o yaml > backup-$(date +%Y%m%d).yaml

# Backup Helm release
helm get values my-gateway -n gateway > values-backup-$(date +%Y%m%d).yaml
helm get manifest my-gateway -n gateway > manifest-backup-$(date +%Y%m%d).yaml

# Store backups in version control or S3
aws s3 cp backup-$(date +%Y%m%d).yaml s3://gateway-backups/
```

**Automated Backup (Cron):**

```bash
# Create CronJob for daily backups
kubectl apply -f - <<EOF
apiVersion: batch/v1
kind: CronJob
metadata:
  name: gateway-backup
  namespace: gateway
spec:
  schedule: "0 2 * * *"  # 2 AM daily
  jobTemplate:
    spec:
      template:
        spec:
          containers:
          - name: backup
            image: bitnami/kubectl:latest
            command:
            - /bin/bash
            - -c
            - |
              kubectl get all,configmap,secret,ingress,hpa -n gateway -o yaml > /backup/gateway-\$(date +%Y%m%d).yaml
              # Upload to S3 or backup location
          restartPolicy: OnFailure
EOF
```

### Recovery Procedures

**From Backup:**

```bash
# 1. Restore from backup file
kubectl apply -f backup-20240101.yaml

# 2. Or restore via Helm
helm install my-gateway ./helm/api-gateway -f values-backup-20240101.yaml

# 3. Verify recovery
kubectl get pods -n gateway
curl https://api.yourdomain.com/health
```

**Disaster Recovery:**

```bash
# 1. Recreate namespace
kubectl create namespace gateway

# 2. Restore secrets (from secure backup)
kubectl apply -f secrets-backup.yaml

# 3. Deploy gateway
helm install my-gateway ./helm/api-gateway -f production-values.yaml

# 4. Verify all services
kubectl get all -n gateway
curl https://api.yourdomain.com/health
```

---

## Security Operations

### Secret Rotation

**JWT Secret Rotation:**

```bash
# 1. Generate new secret
NEW_SECRET=$(openssl rand -base64 32)

# 2. Update secret in Kubernetes
kubectl create secret generic gateway-secrets-new \
  --from-literal=jwt_secret=$NEW_SECRET \
  -n gateway \
  --dry-run=client -o yaml | kubectl apply -f -

# 3. Update deployment to use new secret
kubectl patch deployment api-gateway -n gateway \
  -p '{"spec":{"template":{"spec":{"volumes":[{"name":"secrets","secret":{"secretName":"gateway-secrets-new"}}]}}}}'

# 4. Wait for rollout
kubectl rollout status deployment api-gateway -n gateway

# 5. Verify
kubectl logs -n gateway -l app=api-gateway --tail=10

# 6. Delete old secret after confirmation
kubectl delete secret gateway-secrets -n gateway
```

**TLS Certificate Rotation:**

```bash
# If using cert-manager, certificates rotate automatically

# For manual rotation:
# 1. Generate new certificate
# 2. Create new secret
kubectl create secret tls gateway-tls-new \
  --cert=new-cert.pem \
  --key=new-key.pem \
  -n gateway

# 3. Update deployment
kubectl patch deployment api-gateway -n gateway \
  -p '{"spec":{"template":{"spec":{"volumes":[{"name":"tls-certs","secret":{"secretName":"gateway-tls-new"}}]}}}}'

# 4. Monitor rollout
kubectl rollout status deployment api-gateway -n gateway
```

### Security Scanning

```bash
# Scan Docker image for vulnerabilities
docker scan api-gateway:latest

# Or use trivy
trivy image api-gateway:latest

# Scan running pods
kubectl get pods -n gateway -o jsonpath='{.items[*].spec.containers[*].image}' | \
  xargs -n1 trivy image
```

### Access Auditing

```bash
# Review RBAC permissions
kubectl describe role api-gateway-role -n gateway
kubectl describe rolebinding api-gateway-rolebinding -n gateway

# Check who can access gateway resources
kubectl auth can-i --list --namespace gateway
```

---

## Performance Optimization

### Performance Tuning Checklist

1. **Resource Allocation**
   ```bash
   # Check current resource usage
   kubectl top pods -n gateway

   # Adjust resources if needed
   kubectl edit deployment api-gateway -n gateway
   ```

2. **Connection Pooling**
   - Verify connection pool settings in configuration
   - Monitor connection pool metrics

3. **Rate Limiting**
   - Review rate limit settings
   - Adjust based on traffic patterns

4. **Caching**
   - Enable response caching where appropriate
   - Monitor cache hit rates

5. **Circuit Breaker Tuning**
   ```yaml
   circuit_breaker:
     failure_threshold: 5  # Adjust based on backend reliability
     success_threshold: 2
     timeout_secs: 60
   ```

### Load Testing

```bash
# Run load test
cd scripts/loadtest

# Basic load test
./wrk-basic.sh

# With authentication
./wrk-auth.sh

# Comprehensive k6 test
k6 run k6-load-test.js

# Analyze results
# - Check latency percentiles
# - Verify error rates
# - Monitor resource usage during test
```

---

## Troubleshooting Guide

### High CPU Usage

**Diagnosis:**
```bash
kubectl top pods -n gateway
kubectl exec -it <pod-name> -n gateway -- top
```

**Solutions:**
- Scale horizontally: `kubectl scale deployment api-gateway -n gateway --replicas=<N>`
- Increase CPU limits in deployment
- Review slow routes/backends
- Check for infinite loops in configuration

### High Memory Usage

**Diagnosis:**
```bash
kubectl top pods -n gateway
kubectl describe pod <pod-name> -n gateway
```

**Solutions:**
- Check for memory leaks (review logs)
- Increase memory limits
- Scale horizontally
- Review caching configuration

### Slow Responses

**Diagnosis:**
```bash
# Check latency metrics
curl https://api.yourdomain.com/metrics | grep duration

# Check backend response times
kubectl logs -n gateway -l app=api-gateway --tail=100 | grep -i latency
```

**Solutions:**
- Identify slow backends
- Adjust timeout settings
- Enable caching
- Add circuit breakers
- Scale backend services

### Certificate Issues

**Diagnosis:**
```bash
# Check certificate validity
kubectl get secret gateway-tls -n gateway -o jsonpath='{.data.tls\.crt}' | \
  base64 -d | openssl x509 -text -noout

# If using cert-manager
kubectl describe certificate gateway-tls -n gateway
kubectl get certificaterequest -n gateway
```

**Solutions:**
- Renew certificate if expired
- Check cert-manager configuration
- Verify DNS records for ACME challenges

---

## Maintenance Windows

### Planned Maintenance Procedure

1. **Pre-Maintenance (T-24h)**
   ```bash
   # Notify users
   # Create backup
   kubectl get all,configmap,secret -n gateway -o yaml > pre-maintenance-backup.yaml

   # Verify backup
   yamllint pre-maintenance-backup.yaml
   ```

2. **During Maintenance**
   ```bash
   # Scale down (if full outage acceptable)
   kubectl scale deployment api-gateway -n gateway --replicas=0

   # Perform maintenance tasks
   # ...

   # Scale back up
   kubectl scale deployment api-gateway -n gateway --replicas=5
   ```

3. **Post-Maintenance**
   ```bash
   # Verify health
   kubectl get pods -n gateway
   curl https://api.yourdomain.com/health

   # Monitor metrics for 30 minutes
   # Check error rates
   # Review logs
   ```

### Zero-Downtime Updates

```bash
# Use rolling updates (default)
helm upgrade my-gateway ./helm/api-gateway -f production-values.yaml

# Monitor rollout
kubectl rollout status deployment api-gateway -n gateway

# Rollback if issues detected
helm rollback my-gateway
```

---

## Contact & Escalation

### Escalation Path

1. **On-Call Engineer** - First responder
2. **Team Lead** - P0/P1 incidents
3. **Platform Team** - Infrastructure issues
4. **Security Team** - Security incidents

### Communication Channels

- **Incidents**: #incidents (Slack)
- **Monitoring**: #monitoring-alerts
- **General**: #api-gateway

### SLA Targets

- **Availability**: 99.95% uptime
- **Latency**: p95 < 15ms, p99 < 50ms
- **Error Rate**: < 0.1%

---

For additional help, see [DEPLOYMENT.md](DEPLOYMENT.md) or open an issue on [GitHub](https://github.com/therealutkarshpriyadarshi/gateway/issues).
