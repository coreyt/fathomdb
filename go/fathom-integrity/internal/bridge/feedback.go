package bridge

import (
	"context"
	"fmt"
	"sync"
	"sync/atomic"
	"time"
)

type ResponseCyclePhase string

const (
	PhaseStarted   ResponseCyclePhase = "started"
	PhaseSlow      ResponseCyclePhase = "slow"
	PhaseHeartbeat ResponseCyclePhase = "heartbeat"
	PhaseFinished  ResponseCyclePhase = "finished"
	PhaseFailed    ResponseCyclePhase = "failed"
)

type ResponseCycleEvent struct {
	OperationID     string             `json:"operation_id"`
	OperationKind   string             `json:"operation_kind"`
	Surface         string             `json:"surface"`
	Phase           ResponseCyclePhase `json:"phase"`
	ElapsedMS       int64              `json:"elapsed_ms"`
	SlowThresholdMS int64              `json:"slow_threshold_ms"`
	Metadata        map[string]string  `json:"metadata,omitempty"`
	ErrorCode       string             `json:"error_code,omitempty"`
	ErrorMessage    string             `json:"error_message,omitempty"`
}

type FeedbackConfig struct {
	SlowThreshold     time.Duration
	HeartbeatInterval time.Duration
}

func (c FeedbackConfig) withDefaults() FeedbackConfig {
	if c.SlowThreshold <= 0 {
		c.SlowThreshold = 500 * time.Millisecond
	}
	if c.HeartbeatInterval <= 0 {
		c.HeartbeatInterval = 2 * time.Second
	}
	return c
}

type Observer interface {
	OnEvent(ResponseCycleEvent)
}

type ObserverFunc func(ResponseCycleEvent)

func (f ObserverFunc) OnEvent(event ResponseCycleEvent) {
	f(event)
}

type safeObserver struct {
	inner    Observer
	disabled atomic.Bool
}

func (o *safeObserver) emit(event ResponseCycleEvent) {
	if o == nil || o.inner == nil || o.disabled.Load() {
		return
	}
	defer func() {
		if recover() != nil {
			o.disabled.Store(true)
		}
	}()
	o.inner.OnEvent(event)
}

var operationCounter atomic.Uint64

func RunWithFeedback[T any](
	ctx context.Context,
	surface, operationKind string,
	metadata map[string]string,
	observer Observer,
	config FeedbackConfig,
	operation func(context.Context) (T, error),
) (result T, err error) {
	if observer == nil {
		return operation(ctx)
	}

	config = config.withDefaults()
	startedAt := time.Now()
	operationID := fmt.Sprintf("op-%d", operationCounter.Add(1))
	safe := &safeObserver{inner: observer}
	stop := make(chan struct{})
	var wait sync.WaitGroup

	emit := func(phase ResponseCyclePhase, errorCode, errorMessage string) {
		safe.emit(ResponseCycleEvent{
			OperationID:     operationID,
			OperationKind:   operationKind,
			Surface:         surface,
			Phase:           phase,
			ElapsedMS:       time.Since(startedAt).Milliseconds(),
			SlowThresholdMS: config.SlowThreshold.Milliseconds(),
			Metadata:        cloneMetadata(metadata),
			ErrorCode:       errorCode,
			ErrorMessage:    errorMessage,
		})
	}

	emit(PhaseStarted, "", "")

	wait.Add(1)
	go func() {
		defer wait.Done()

		timer := time.NewTimer(config.SlowThreshold)
		defer timer.Stop()

		select {
		case <-stop:
			return
		case <-ctx.Done():
			return
		case <-timer.C:
			emit(PhaseSlow, "", "")
		}

		ticker := time.NewTicker(config.HeartbeatInterval)
		defer ticker.Stop()
		for {
			select {
			case <-stop:
				return
			case <-ctx.Done():
				return
			case <-ticker.C:
				emit(PhaseHeartbeat, "", "")
			}
		}
	}()

	var panicValue any
	func() {
		defer func() {
			panicValue = recover()
		}()
		result, err = operation(ctx)
	}()

	close(stop)
	wait.Wait()

	if panicValue != nil {
		emit(PhaseFailed, "panic", "operation panicked")
		panic(panicValue)
	}
	if err != nil {
		emit(PhaseFailed, fmt.Sprintf("%T", err), err.Error())
		return result, err
	}
	emit(PhaseFinished, "", "")
	return result, nil
}

func cloneMetadata(metadata map[string]string) map[string]string {
	if len(metadata) == 0 {
		return map[string]string{}
	}
	cloned := make(map[string]string, len(metadata))
	for key, value := range metadata {
		cloned[key] = value
	}
	return cloned
}
