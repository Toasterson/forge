# Kubernetes Deployment

Forge includes a Helm chart at `charts/forged/` for Kubernetes deployments.

## Prerequisites

- Kubernetes 1.24+
- Helm 3
- PostgreSQL (managed or in-cluster)
- SeaweedFS (managed or in-cluster)

## Installation

```bash
helm install forged charts/forged/ \
  --set postgres.url=postgresql://forged:forged@postgres:5432/forged \
  --set seaweedfs.masterUrl=http://seaweedfs-master:9333
```

## Configuration

Override values in `charts/forged/values.yaml` or pass them with `--set`:

```yaml
# charts/forged/values.yaml
replicaCount: 1

image:
  repository: ghcr.io/openflowlabs/forged
  tag: latest
  pullPolicy: IfNotPresent

service:
  type: ClusterIP
  port: 50051

postgres:
  url: postgresql://forged:forged@postgres:5432/forged

seaweedfs:
  masterUrl: http://seaweedfs-master:9333

resources:
  requests:
    memory: "256Mi"
    cpu: "250m"
  limits:
    memory: "1Gi"
    cpu: "1000m"

env:
  RUST_LOG: "forged=info,warn"
```

## Scaling

Forge is stateless (all state is in PostgreSQL and SeaweedFS), so you can scale horizontally:

```bash
kubectl scale deployment forged --replicas=3
```

## Upgrading

```bash
helm upgrade forged charts/forged/ --set image.tag=v0.2.0
```

## Uninstalling

```bash
helm uninstall forged
```

This removes the Forge deployment but preserves data in PostgreSQL and SeaweedFS.
