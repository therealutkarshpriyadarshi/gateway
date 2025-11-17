use gateway::circuit_breaker::{CircuitBreakerConfig, CircuitBreakerService, CircuitState};
use std::time::Duration;
use tokio::time::sleep;

#[tokio::test]
async fn test_circuit_breaker_integration() {
    let config = CircuitBreakerConfig {
        failure_threshold: 3,
        success_threshold: 2,
        timeout_secs: 1,
        half_open_requests: 2,
        request_timeout_secs: 30,
    };

    let service = CircuitBreakerService::new(config);
    let backend = "http://test-backend:8080";

    // Initially circuit should be closed
    assert_eq!(service.state(backend).await, CircuitState::Closed);
    assert!(service.can_proceed(backend).await);

    // Record failures to open the circuit
    for _ in 0..3 {
        assert!(service.can_proceed(backend).await);
        service.record_failure(backend).await;
    }

    // Circuit should be open now
    assert_eq!(service.state(backend).await, CircuitState::Open);
    assert!(!service.can_proceed(backend).await);

    // Wait for timeout to transition to half-open
    sleep(Duration::from_secs(2)).await;

    // Should allow probe requests now
    assert!(service.can_proceed(backend).await);
    assert_eq!(service.state(backend).await, CircuitState::HalfOpen);

    // Record successes to close the circuit
    service.record_success(backend).await;
    assert!(service.can_proceed(backend).await);
    service.record_success(backend).await;

    // Circuit should be closed again
    assert_eq!(service.state(backend).await, CircuitState::Closed);
    assert!(service.can_proceed(backend).await);
}

#[tokio::test]
async fn test_circuit_breaker_timeout_tracking() {
    let config = CircuitBreakerConfig {
        failure_threshold: 2,
        success_threshold: 2,
        timeout_secs: 1,
        half_open_requests: 2,
        request_timeout_secs: 30,
    };

    let service = CircuitBreakerService::new(config);
    let backend = "http://timeout-backend:8080";

    // Record timeouts
    assert!(service.can_proceed(backend).await);
    service.record_timeout(backend).await;

    assert!(service.can_proceed(backend).await);
    service.record_timeout(backend).await;

    // Circuit should be open
    assert_eq!(service.state(backend).await, CircuitState::Open);

    // Check metrics
    let metrics = service.metrics(backend).await.unwrap();
    assert_eq!(metrics.timeout_count, 2);
    assert_eq!(metrics.failed_requests, 2);
    assert_eq!(metrics.circuit_opened_count, 1);
}

#[tokio::test]
async fn test_multiple_backends() {
    let config = CircuitBreakerConfig {
        failure_threshold: 2,
        ..Default::default()
    };

    let service = CircuitBreakerService::new(config);

    let backend1 = "http://backend1:8080";
    let backend2 = "http://backend2:8080";

    // Backend 1: Keep it closed with successes
    assert!(service.can_proceed(backend1).await);
    service.record_success(backend1).await;

    // Backend 2: Open it with failures
    for _ in 0..2 {
        assert!(service.can_proceed(backend2).await);
        service.record_failure(backend2).await;
    }

    // Backend 1 should still be closed
    assert_eq!(service.state(backend1).await, CircuitState::Closed);
    assert!(service.can_proceed(backend1).await);

    // Backend 2 should be open
    assert_eq!(service.state(backend2).await, CircuitState::Open);
    assert!(!service.can_proceed(backend2).await);

    // Check all metrics
    let all_metrics = service.all_metrics().await;
    assert_eq!(all_metrics.len(), 2);

    // Find backend1
    let backend1_data = all_metrics
        .iter()
        .find(|(name, _, _)| name == backend1)
        .unwrap();
    assert_eq!(backend1_data.2, CircuitState::Closed);
    assert_eq!(backend1_data.1.successful_requests, 1);

    // Find backend2
    let backend2_data = all_metrics
        .iter()
        .find(|(name, _, _)| name == backend2)
        .unwrap();
    assert_eq!(backend2_data.2, CircuitState::Open);
    assert_eq!(backend2_data.1.failed_requests, 2);
}

#[tokio::test]
async fn test_half_open_failure_reopens_circuit() {
    let config = CircuitBreakerConfig {
        failure_threshold: 2,
        success_threshold: 2,
        timeout_secs: 1,
        half_open_requests: 3,
        request_timeout_secs: 30,
    };

    let service = CircuitBreakerService::new(config);
    let backend = "http://flaky-backend:8080";

    // Open the circuit
    for _ in 0..2 {
        assert!(service.can_proceed(backend).await);
        service.record_failure(backend).await;
    }
    assert_eq!(service.state(backend).await, CircuitState::Open);

    // Wait for timeout to transition to half-open
    sleep(Duration::from_secs(2)).await;
    assert!(service.can_proceed(backend).await);
    assert_eq!(service.state(backend).await, CircuitState::HalfOpen);

    // Fail in half-open state - should reopen
    service.record_failure(backend).await;
    assert_eq!(service.state(backend).await, CircuitState::Open);
}

#[tokio::test]
async fn test_circuit_breaker_metrics_accuracy() {
    let config = CircuitBreakerConfig {
        failure_threshold: 5,
        success_threshold: 3,
        timeout_secs: 1,
        half_open_requests: 2,
        request_timeout_secs: 30,
    };

    let service = CircuitBreakerService::new(config);
    let backend = "http://metrics-backend:8080";

    // Make various requests
    assert!(service.can_proceed(backend).await);
    service.record_success(backend).await;

    assert!(service.can_proceed(backend).await);
    service.record_success(backend).await;

    assert!(service.can_proceed(backend).await);
    service.record_failure(backend).await;

    assert!(service.can_proceed(backend).await);
    service.record_timeout(backend).await;

    // Try to proceed but get rejected (shouldn't happen yet)
    assert!(service.can_proceed(backend).await);

    let metrics = service.metrics(backend).await.unwrap();
    assert_eq!(metrics.total_requests, 5);
    assert_eq!(metrics.successful_requests, 2);
    assert_eq!(metrics.failed_requests, 2); // failure + timeout
    assert_eq!(metrics.timeout_count, 1);
    assert_eq!(metrics.rejected_requests, 0);
    assert_eq!(metrics.circuit_opened_count, 0);
}
