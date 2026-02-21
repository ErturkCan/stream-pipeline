use stream_pipeline::{
    OverflowPolicy, PipelineBuilder, Stage, PassthroughStage, FilterStage,
    MapStage, Result as PipelineResult,
};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

#[test]
fn test_single_stage_pipeline() {
    let pipeline = PipelineBuilder::new()
        .add_stage("passthrough", 10, OverflowPolicy::Block)
        .build()
        .expect("Pipeline build failed");

    let input = pipeline.input();

    let running = pipeline
        .start(|_| Box::new(PassthroughStage))
        .expect("Pipeline start failed");

    // Push some data
    for i in 0..5 {
        let data = vec![i as u8];
        input.push(data).expect("Push failed");
    }

    // Small delay to allow processing
    std::thread::sleep(Duration::from_millis(100));

    running.wait().expect("Wait failed");
}

#[test]
fn test_multi_stage_pipeline() {
    let pipeline = PipelineBuilder::new()
        .add_stage("stage1", 10, OverflowPolicy::Block)
        .add_stage("stage2", 10, OverflowPolicy::Block)
        .add_stage("stage3", 10, OverflowPolicy::Block)
        .build()
        .expect("Pipeline build failed");

    let input = pipeline.input();

    let running = pipeline
        .start(|_| Box::new(PassthroughStage))
        .expect("Pipeline start failed");

    // Push data
    for i in 0..10 {
        let data = vec![i as u8];
        input.push(data).expect("Push failed");
    }

    std::thread::sleep(Duration::from_millis(150));
    running.wait_timeout(Duration::from_secs(30)).expect("Wait failed");
}

#[test]
fn test_filter_stage() {
    let pipeline = PipelineBuilder::new()
        .add_stage("filter", 20, OverflowPolicy::Block)
        .build()
        .expect("Pipeline build failed");

    let input = pipeline.input();

    let running = pipeline
        .start(|_| {
            Box::new(FilterStage::new("even_filter", |data| {
                data[0] % 2 == 0
            }))
        })
        .expect("Pipeline start failed");

    // Push numbers, only evens should pass
    for i in 0..10 {
        input.push(vec![i]).expect("Push failed");
    }

    std::thread::sleep(Duration::from_millis(150));
    running.wait_timeout(Duration::from_secs(30)).expect("Wait failed");
}

#[test]
fn test_map_stage() {
    let pipeline = PipelineBuilder::new()
        .add_stage("map", 20, OverflowPolicy::Block)
        .build()
        .expect("Pipeline build failed");

    let input = pipeline.input();

    let running = pipeline
        .start(|_| {
            Box::new(MapStage::new("double", |mut data| {
                data[0] *= 2;
                Ok(data)
            }))
        })
        .expect("Pipeline start failed");

    for i in 0..5 {
        input.push(vec![i]).expect("Push failed");
    }

    std::thread::sleep(Duration::from_millis(150));
    running.wait_timeout(Duration::from_secs(30)).expect("Wait failed");
}

#[test]
fn test_drop_policy() {
    let pipeline = PipelineBuilder::new()
        .add_stage("drop_buffer", 3, OverflowPolicy::DropOldest)
        .build()
        .expect("Pipeline build failed");

    let input = pipeline.input();

    let running = pipeline
        .start(|_| Box::new(PassthroughStage))
        .expect("Pipeline start failed");

    // Push more items than buffer capacity
    for i in 0..10 {
        let _ = input.push(vec![i as u8]);
    }

    std::thread::sleep(Duration::from_millis(150));
    running.wait_timeout(Duration::from_secs(30)).expect("Wait failed");
}

#[test]
#[ignore]
fn test_backpressure_activation() {
    let counter = Arc::new(AtomicUsize::new(0));

    let pipeline = PipelineBuilder::new()
        .add_stage("source", 20, OverflowPolicy::Block)
        .add_stage("sink", 5, OverflowPolicy::Block)
        .with_backpressure(true)
        .build()
        .expect("Pipeline build failed");

    let input = pipeline.input();
    let counter_clone = Arc::clone(&counter);

    let running = pipeline
        .start(|stage_idx| {
            if stage_idx == 0 {
                // Source: produces data quickly
                Box::new(PassthroughStage)
            } else {
                // Sink: slow consumer
                let count = Arc::clone(&counter_clone);
                Box::new(MapStage::new("slow_sink", move |data| {
                    std::thread::sleep(Duration::from_millis(10));
                    count.fetch_add(1, Ordering::Relaxed);
                    Ok(data)
                }))
            }
        })
        .expect("Pipeline start failed");

    // Try to push more than sink can handle
    for i in 0..20 {
        let _ = input.push(vec![i as u8]);
    }

    std::thread::sleep(Duration::from_millis(500));
    running.wait_timeout(Duration::from_secs(30)).expect("Wait failed");

    // Some items should have been processed
    assert!(counter.load(Ordering::Relaxed) > 0);
}

#[test]
#[ignore]
fn test_metrics_collection() {
    let pipeline = PipelineBuilder::new()
        .add_stage("metrics_test", 20, OverflowPolicy::Block)
        .build()
        .expect("Pipeline build failed");

    let input = pipeline.input();
    let metrics = pipeline.stage_metrics(0).expect("Metrics not found").clone();

    let running = pipeline
        .start(|_| Box::new(PassthroughStage))
        .expect("Pipeline start failed");

    // Push data
    for i in 0..100 {
        input.push(vec![i as u8]).expect("Push failed");
    }

    std::thread::sleep(Duration::from_millis(300));
    running.wait_timeout(Duration::from_secs(30)).expect("Wait failed");

    let snapshot = metrics.snapshot();
    assert!(snapshot.throughput_mps > 0.0);
}

#[test]
#[ignore]
fn test_custom_stage() {
    struct CountingStage {
        count: usize,
    }

    impl Stage for CountingStage {
        fn process(&mut self, input: Vec<u8>) -> PipelineResult<Vec<Vec<u8>>> {
            self.count += 1;
            Ok(vec![input])
        }

        fn name(&self) -> &str {
            "counter"
        }
    }

    let pipeline = PipelineBuilder::new()
        .add_stage("counter", 20, OverflowPolicy::Block)
        .build()
        .expect("Pipeline build failed");

    let input = pipeline.input();

    let running = pipeline
        .start(|_| Box::new(CountingStage { count: 0 }))
        .expect("Pipeline start failed");

    for i in 0..50 {
        input.push(vec![i as u8]).expect("Push failed");
    }

    std::thread::sleep(Duration::from_millis(200));
    running.wait_timeout(Duration::from_secs(30)).expect("Wait failed");
}

#[test]
fn test_pipeline_with_backpressure_release() {
    let pipeline = PipelineBuilder::new()
        .add_stage("bp_test", 10, OverflowPolicy::Block)
        .with_backpressure(true)
        .build()
        .expect("Pipeline build failed");

    let input = pipeline.input();

    let running = pipeline
        .start(|_| Box::new(PassthroughStage))
        .expect("Pipeline start failed");

    // Push some data
    for i in 0..5 {
        input.push(vec![i as u8]).expect("Push failed");
    }

    std::thread::sleep(Duration::from_millis(150));
    running.wait_timeout(Duration::from_secs(30)).expect("Wait failed");
}
