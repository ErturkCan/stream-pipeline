use criterion::{black_box, criterion_group, criterion_main, Criterion};
use stream_pipeline::{MapStage, OverflowPolicy, PipelineBuilder, PassthroughStage};
use std::time::Duration;

fn benchmark_backpressure_slow_consumer(c: &mut Criterion) {
    c.bench_function("backpressure_slow_consumer_1000_msgs", |b| {
        b.iter(|| {
            let pipeline = PipelineBuilder::new()
                .add_stage("producer", 500, OverflowPolicy::Block)
                .add_stage("consumer", 100, OverflowPolicy::Block)
                .with_backpressure(true)
                .build()
                .expect("Build failed");

            let input = pipeline.input();

            let running = pipeline
                .start(|stage_idx| {
                    if stage_idx == 1 {
                        // Slow consumer
                        Box::new(MapStage::new("slow", |data| {
                            std::thread::sleep(Duration::from_micros(100));
                            Ok(data)
                        }))
                    } else {
                        Box::new(PassthroughStage)
                    }
                })
                .expect("Start failed");

            // Push data rapidly
            for i in 0..1000 {
                let data = vec![i as u8; 64];
                let _ = input.push(black_box(data));
            }

            std::thread::sleep(Duration::from_millis(2000));
            let _ = running.wait();
        });
    });
}

fn benchmark_without_backpressure(c: &mut Criterion) {
    c.bench_function("no_backpressure_slow_consumer_1000_msgs", |b| {
        b.iter(|| {
            let pipeline = PipelineBuilder::new()
                .add_stage("producer", 500, OverflowPolicy::Block)
                .add_stage("consumer", 100, OverflowPolicy::Block)
                .with_backpressure(false)
                .build()
                .expect("Build failed");

            let input = pipeline.input();

            let running = pipeline
                .start(|stage_idx| {
                    if stage_idx == 1 {
                        Box::new(MapStage::new("slow", |data| {
                            std::thread::sleep(Duration::from_micros(100));
                            Ok(data)
                        }))
                    } else {
                        Box::new(PassthroughStage)
                    }
                })
                .expect("Start failed");

            for i in 0..1000 {
                let data = vec![i as u8; 64];
                let _ = input.push(black_box(data));
            }

            std::thread::sleep(Duration::from_millis(2000));
            let _ = running.wait();
        });
    });
}

fn benchmark_drop_policy_high_load(c: &mut Criterion) {
    c.bench_function("drop_policy_high_load_2000_msgs", |b| {
        b.iter(|| {
            let pipeline = PipelineBuilder::new()
                .add_stage("producer", 200, OverflowPolicy::DropOldest)
                .build()
                .expect("Build failed");

            let input = pipeline.input();

            let running = pipeline
                .start(|_| {
                    Box::new(MapStage::new("slow", |data| {
                        std::thread::sleep(Duration::from_micros(100));
                        Ok(data)
                    }))
                })
                .expect("Start failed");

            for i in 0..2000 {
                let data = vec![i as u8; 64];
                let _ = input.push(black_box(data));
            }

            std::thread::sleep(Duration::from_millis(2000));
            let _ = running.wait();
        });
    });
}

criterion_group!(
    name = benches;
    config = Criterion::default()
        .measurement_time(Duration::from_secs(15))
        .sample_size(20);
    targets = benchmark_backpressure_slow_consumer, benchmark_without_backpressure, benchmark_drop_policy_high_load
);
criterion_main!(benches);
