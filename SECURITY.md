# Security Best Practices

This document outlines security best practices for deploying and operating the API Gateway.

## Table of Contents

1. [Security Overview](#security-overview)
2. [TLS/mTLS Configuration](#tlsmtls-configuration)
3. [Authentication & Authorization](#authentication--authorization)
4. [Secrets Management](#secrets-management)
5. [Network Security](#network-security)
6. [Rate Limiting & DDoS Protection](#rate-limiting--ddos-protection)
7. [Container Security](#container-security)
8. [Kubernetes Security](#kubernetes-security)
9. [Monitoring & Auditing](#monitoring--auditing)
10. [Incident Response](#incident-response)
11. [Compliance](#compliance)

---

## Security Overview

### Security Principles

1. **Defense in Depth** - Multiple layers of security
2. **Least Privilege** - Minimal necessary permissions
3. **Zero Trust** - Never trust, always verify
4. **Security by Default** - Secure default configurations
5. **Regular Updates** - Keep dependencies current

### Threat Model

**Protected Against:**
- Unauthorized access
- Man-in-the-middle attacks
- DDoS attacks
- Injection attacks
- Certificate spoofing

**Attack Surface:**
- External API endpoints
- Internal cluster communication
- Configuration storage
- Secrets storage
- Container images

---

## TLS/mTLS Configuration

### TLS Requirements

**Production Checklist:**

- [ ] TLS 1.2+ only (disable TLS 1.0, 1.1)
- [ ] Strong cipher suites only
- [ ] Valid certificates from trusted CA
- [ ] Certificate expiration monitoring
- [ ] Regular certificate rotation
- [ ] HSTS enabled

### TLS Configuration

**In gateway configuration:**

```yaml
tls:
  enabled: true
  cert_path: "/etc/gateway/tls/tls.crt"
  key_path: "/etc/gateway/tls/tls.key"
  # Use only strong cipher suites
  # Configured in rustls by default
```

### Mutual TLS (mTLS)

**For high-security environments:**

```yaml
tls:
  enabled: true
  cert_path: "/etc/gateway/tls/tls.crt"
  key_path: "/etc/gateway/tls/tls.key"
  enable_mtls: true
  ca_cert_path: "/etc/gateway/tls/ca.crt"
  require_client_cert: true
```

**Client Certificate Generation:**

```bash
# Generate client key
openssl genrsa -out client.key 2048

# Generate CSR
openssl req -new -key client.key -out client.csr \
  -subj "/CN=client.example.com"

# Sign with CA
openssl x509 -req -in client.csr -CA ca.crt -CAkey ca.key \
  -CAcreateserial -out client.crt -days 365

# Test connection
curl --cert client.crt --key client.key --cacert ca.crt \
  https://api.example.com/health
```

### Certificate Management

**Using cert-manager (recommended):**

```yaml
apiVersion: cert-manager.io/v1
kind: Certificate
metadata:
  name: gateway-tls
  namespace: gateway
spec:
  secretName: gateway-tls
  duration: 2160h # 90 days
  renewBefore: 720h # 30 days
  issuerRef:
    name: letsencrypt-prod
    kind: ClusterIssuer
  dnsNames:
    - api.example.com
    - gateway.example.com
```

**Certificate Rotation:**

```bash
# Automated with cert-manager

# Manual rotation:
# 1. Generate new certificate
# 2. Create new secret
kubectl create secret tls gateway-tls-new \
  --cert=new-cert.pem --key=new-key.pem -n gateway

# 3. Update deployment (triggers rolling update)
kubectl patch deployment api-gateway -n gateway \
  -p '{"spec":{"template":{"spec":{"volumes":[{"name":"tls-certs","secret":{"secretName":"gateway-tls-new"}}]}}}}'
```

---

## Authentication & Authorization

### JWT Security

**Best Practices:**

1. **Use Strong Secrets**
   ```bash
   # Generate secure secret (256-bit)
   openssl rand -base64 32
   ```

2. **RS256 for Production**
   - Asymmetric keys
   - Public key for verification
   - Private key only in auth service

3. **Token Expiration**
   - Short-lived tokens (15-60 minutes)
   - Refresh tokens for long sessions

4. **Validate All Claims**
   ```yaml
   auth:
     jwt:
       algorithm: "RS256"
       public_key: |
         -----BEGIN PUBLIC KEY-----
         ...
         -----END PUBLIC KEY-----
       issuer: "https://auth.example.com"
       audience: "https://api.example.com"
   ```

### API Key Security

**Best Practices:**

1. **Key Format**
   - Prefixed (e.g., `sk_live_...`, `sk_test_...`)
   - Long and random (32+ bytes)
   - Include checksum for validation

2. **Key Storage**
   - Never in plain text
   - Use Redis with encryption at rest
   - Rotate keys regularly

3. **Key Scoping**
   - Limit by IP address
   - Limit by rate
   - Limit by routes/permissions

4. **Key Rotation**
   ```bash
   # Generate new key
   NEW_KEY=$(openssl rand -hex 32)

   # Add to Redis
   redis-cli SET gateway:apikey:$NEW_KEY '{"description":"New key","created":"2024-01-01"}'

   # Deprecate old key
   redis-cli EXPIRE gateway:apikey:$OLD_KEY 2592000  # 30 days
   ```

### Authorization

**Route-Level Authorization:**

```yaml
routes:
  - path: "/api/admin/*path"
    backend: "http://admin-service:3000"
    auth:
      required: true
      methods: ["jwt"]
      # Additional authorization checks in backend
```

**IP Whitelisting:**

```yaml
ip_filter:
  enabled: true
  whitelist:
    - "10.0.0.0/8"      # Internal network
    - "203.0.113.0/24"  # Partner network
  blacklist:
    - "192.0.2.0/24"    # Known malicious
```

---

## Secrets Management

### Secrets Storage

**Kubernetes Secrets (Basic):**

```bash
# Create secret
kubectl create secret generic gateway-secrets \
  --from-literal=jwt_secret="$(openssl rand -base64 32)" \
  -n gateway

# Encrypt at rest (enable in cluster)
# /etc/kubernetes/encryption-config.yaml
```

**External Secrets Operator (Recommended):**

```yaml
apiVersion: external-secrets.io/v1beta1
kind: ExternalSecret
metadata:
  name: gateway-secrets
  namespace: gateway
spec:
  refreshInterval: 1h
  secretStoreRef:
    name: aws-secrets-manager
    kind: SecretStore
  target:
    name: gateway-secrets
  data:
    - secretKey: jwt_secret
      remoteRef:
        key: prod/gateway/jwt_secret
```

**Supported Secret Stores:**
- AWS Secrets Manager
- Azure Key Vault
- Google Secret Manager
- HashiCorp Vault

### Secret References

**In Configuration:**

```yaml
auth:
  jwt:
    secret: "secret://jwt_secret"  # Load from secret manager
    algorithm: "HS256"

api_key:
  redis:
    url: "secret://redis_url"
```

### Secret Rotation

**Automated Rotation:**

```bash
# Using External Secrets Operator
# Secrets are automatically rotated based on refreshInterval

# Manual verification
kubectl get externalsecret gateway-secrets -n gateway -o yaml
```

---

## Network Security

### Network Policies

**Default Deny:**

```yaml
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: default-deny-all
  namespace: gateway
spec:
  podSelector: {}
  policyTypes:
  - Ingress
  - Egress
```

**Allow Gateway Traffic:**

```yaml
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: gateway-network-policy
  namespace: gateway
spec:
  podSelector:
    matchLabels:
      app: api-gateway
  policyTypes:
  - Ingress
  - Egress
  ingress:
  - from:
    - namespaceSelector:
        matchLabels:
          name: ingress-nginx
    ports:
    - protocol: TCP
      port: 8080
  - from:
    - podSelector:
        matchLabels:
          app: prometheus
    ports:
    - protocol: TCP
      port: 8080  # Metrics
  egress:
  - to:
    - namespaceSelector: {}
    ports:
    - protocol: TCP
      port: 53  # DNS
    - protocol: UDP
      port: 53
  - to:
    - namespaceSelector:
        matchLabels:
          name: backend-services
    ports:
    - protocol: TCP
      port: 3000
      port: 3001
  - to:
    - podSelector:
        matchLabels:
          app: redis
    ports:
    - protocol: TCP
      port: 6379
```

### Service Mesh Integration

**With Istio:**

```yaml
apiVersion: security.istio.io/v1beta1
kind: PeerAuthentication
metadata:
  name: gateway-mtls
  namespace: gateway
spec:
  mtls:
    mode: STRICT

---
apiVersion: security.istio.io/v1beta1
kind: AuthorizationPolicy
metadata:
  name: gateway-authz
  namespace: gateway
spec:
  selector:
    matchLabels:
      app: api-gateway
  rules:
  - from:
    - source:
        namespaces: ["ingress-nginx"]
  - to:
    - operation:
        methods: ["GET"]
        paths: ["/health", "/metrics"]
```

---

## Rate Limiting & DDoS Protection

### Rate Limiting Configuration

**Multi-Layer Rate Limiting:**

```yaml
rate_limiting:
  enabled: true
  backend: "redis"  # Distributed
  redis:
    url: "redis://redis:6379"
    prefix: "ratelimit:"

  # Global limits
  limits:
    # Per IP
    - dimension: "ip"
      rate: 1000
      per: 60  # 1000 req/min per IP

    # Per user (JWT sub claim)
    - dimension: "user"
      rate: 10000
      per: 60

    # Per API key
    - dimension: "api_key"
      rate: 5000
      per: 60

  # Route-specific overrides
  route_limits:
    "/api/expensive":
      - dimension: "ip"
        rate: 10
        per: 60  # More restrictive
```

### DDoS Protection

**Cloudflare Integration:**

```yaml
# Use Cloudflare in front of gateway
# - DDoS protection
# - WAF rules
# - Rate limiting
# - Bot detection
```

**Ingress Rate Limiting:**

```yaml
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: api-gateway-ingress
  annotations:
    nginx.ingress.kubernetes.io/limit-rps: "100"
    nginx.ingress.kubernetes.io/limit-burst-multiplier: "5"
    nginx.ingress.kubernetes.io/limit-whitelist: "10.0.0.0/8"
```

---

## Container Security

### Image Security

**Best Practices:**

1. **Use Official Base Images**
   ```dockerfile
   FROM rust:1.75-slim as builder
   FROM debian:bookworm-slim  # Minimal runtime
   ```

2. **Non-Root User**
   ```dockerfile
   RUN useradd -m -u 1000 gateway
   USER gateway
   ```

3. **Read-Only Filesystem**
   ```yaml
   securityContext:
     readOnlyRootFilesystem: true
   ```

4. **No Privileged Containers**
   ```yaml
   securityContext:
     allowPrivilegeEscalation: false
     capabilities:
       drop:
       - ALL
   ```

5. **Image Scanning**
   ```bash
   # Scan for vulnerabilities
   trivy image api-gateway:latest

   # Fail build on high/critical vulnerabilities
   trivy image --severity HIGH,CRITICAL --exit-code 1 api-gateway:latest
   ```

### Supply Chain Security

**Image Signing:**

```bash
# Sign images with Sigstore/cosign
cosign sign --key cosign.key api-gateway:latest

# Verify signatures
cosign verify --key cosign.pub api-gateway:latest
```

**Software Bill of Materials (SBOM):**

```bash
# Generate SBOM
syft api-gateway:latest -o cyclonedx-json > sbom.json

# Attach to image
cosign attach sbom --sbom sbom.json api-gateway:latest
```

---

## Kubernetes Security

### Pod Security Standards

**Restricted Policy:**

```yaml
apiVersion: v1
kind: Pod
metadata:
  name: api-gateway
spec:
  securityContext:
    runAsNonRoot: true
    runAsUser: 1000
    fsGroup: 1000
    seccompProfile:
      type: RuntimeDefault

  containers:
  - name: gateway
    securityContext:
      allowPrivilegeEscalation: false
      capabilities:
        drop:
        - ALL
      readOnlyRootFilesystem: true
```

### RBAC Configuration

**Minimal Permissions:**

```yaml
apiVersion: rbac.authorization.k8s.io/v1
kind: Role
metadata:
  name: api-gateway-role
  namespace: gateway
rules:
# Read-only access to configs and secrets
- apiGroups: [""]
  resources: ["configmaps", "secrets"]
  verbs: ["get", "list", "watch"]
# Service discovery
- apiGroups: [""]
  resources: ["services", "endpoints"]
  verbs: ["get", "list", "watch"]
# No write permissions
```

### Admission Controllers

**Pod Security Admission:**

```yaml
apiVersion: v1
kind: Namespace
metadata:
  name: gateway
  labels:
    pod-security.kubernetes.io/enforce: restricted
    pod-security.kubernetes.io/audit: restricted
    pod-security.kubernetes.io/warn: restricted
```

**OPA Gatekeeper Policies:**

```yaml
apiVersion: templates.gatekeeper.sh/v1beta1
kind: ConstraintTemplate
metadata:
  name: k8sblockprivileged
spec:
  # Block privileged containers
  # Block containers running as root
  # Require resource limits
  # etc.
```

---

## Monitoring & Auditing

### Security Monitoring

**Log Security Events:**

```yaml
# In gateway configuration
observability:
  tracing:
    enabled: true
  # Log security events:
  # - Authentication failures
  # - Rate limit violations
  # - Invalid certificates
  # - Suspicious patterns
```

**Security Metrics:**

```promql
# Authentication failures
rate(auth_failures_total[5m])

# Rate limit violations
rate(rate_limit_exceeded_total[5m])

# TLS errors
rate(tls_errors_total[5m])
```

### Audit Logging

**Kubernetes Audit Policy:**

```yaml
apiVersion: audit.k8s.io/v1
kind: Policy
rules:
- level: RequestResponse
  namespaces: ["gateway"]
  verbs: ["create", "update", "patch", "delete"]
  resources:
  - group: ""
    resources: ["secrets", "configmaps"]
```

### Intrusion Detection

**Falco Rules:**

```yaml
- rule: Unauthorized Process in Container
  desc: Detect unexpected process in gateway container
  condition: >
    container.image.repository = "api-gateway" and
    proc.name != gateway
  output: >
    Unauthorized process in gateway container
    (proc=%proc.name user=%user.name container=%container.name)
  priority: WARNING
```

---

## Incident Response

### Security Incident Procedure

**1. Detection**
- Alert fired from monitoring
- Unusual patterns detected
- User report

**2. Initial Response**
- Contain the incident
- Assess impact
- Preserve evidence

**3. Investigation**
```bash
# Collect logs
kubectl logs -n gateway -l app=api-gateway --since=1h > incident-logs.txt

# Check recent config changes
kubectl get events -n gateway --sort-by='.lastTimestamp'

# Review audit logs
# (depends on audit backend)

# Check for compromised secrets
kubectl get secrets -n gateway -o yaml
```

**4. Remediation**
- Patch vulnerabilities
- Rotate compromised secrets
- Update security policies
- Apply fixes

**5. Recovery**
- Restore from known-good state
- Verify system integrity
- Resume normal operations

**6. Post-Incident**
- Root cause analysis
- Update procedures
- Security improvements

### Emergency Procedures

**Suspected Breach:**

```bash
# 1. Isolate gateway
kubectl scale deployment api-gateway -n gateway --replicas=0

# 2. Rotate all secrets
# (see Secrets Management section)

# 3. Review logs for IoCs
kubectl logs -n gateway -l app=api-gateway --since=24h | grep -i "suspicious|attack|breach"

# 4. Redeploy from known-good state
helm rollback my-gateway <known-good-revision>

# 5. Monitor closely
# (review all security metrics)
```

---

## Compliance

### GDPR Compliance

- **Data Minimization**: Only log necessary data
- **Right to Erasure**: Ability to delete user data from logs
- **Data Protection**: Encryption at rest and in transit
- **Access Controls**: RBAC and authentication

### PCI DSS Compliance

- **Encryption**: TLS 1.2+ for all communications
- **Access Control**: Strong authentication required
- **Logging**: Comprehensive audit trails
- **Vulnerability Management**: Regular scanning and patching

### SOC 2 Compliance

- **Security**: Multi-layer security controls
- **Availability**: High availability configuration
- **Processing Integrity**: Input validation, circuit breakers
- **Confidentiality**: Encryption and access controls
- **Privacy**: Data protection measures

---

## Security Checklist

**Pre-Production:**

- [ ] TLS enabled with valid certificates
- [ ] All secrets stored securely (not in config files)
- [ ] Authentication enabled on all routes
- [ ] Rate limiting configured
- [ ] Network policies in place
- [ ] Pod security standards enforced
- [ ] RBAC configured with least privilege
- [ ] Container images scanned for vulnerabilities
- [ ] Monitoring and alerting configured
- [ ] Incident response plan documented
- [ ] Security testing completed
- [ ] Compliance requirements met

**Ongoing:**

- [ ] Weekly vulnerability scans
- [ ] Monthly secret rotation
- [ ] Quarterly security reviews
- [ ] Regular penetration testing
- [ ] Continuous monitoring
- [ ] Timely patching

---

## Reporting Security Issues

**Do not** open public issues for security vulnerabilities.

Instead, email: security@example.com

Include:
- Description of vulnerability
- Steps to reproduce
- Potential impact
- Suggested fix (if any)

We aim to respond within 24 hours.

---

For additional security guidance, see:
- [OWASP API Security Top 10](https://owasp.org/www-project-api-security/)
- [CIS Kubernetes Benchmark](https://www.cisecurity.org/benchmark/kubernetes)
- [Kubernetes Security Best Practices](https://kubernetes.io/docs/concepts/security/)
