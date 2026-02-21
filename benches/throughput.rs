use criterion::{black_box, criterion_group, criterion_main, Criterion};
use stream_pipeline::{OverflowPolicy, PipelineBuilder, PassthroughStage};
use std::time::Duration;

fn benchmark_single_stage_throughput(c: &mut Criterion) {
    c.bench_function("single_stage_1000_msgs", |b| {
        b.iter(|| {
            let pipeline = PipelineBuilder::new()
                .add_stage("passthrough", 1000, OverflowPolicy::Block)
                .build()
                .expect("Build failed");

            let input = pipeline.input();

            let running = pipeline
                .start(|_| Box::new(PassthroughStage))
                .expect("Start failed");

            // Push 1000 messages
            for i in 0..1000 {
                let data = vec![i as u8; 64]; // 64 bytes per message
                let _ = input.push(black_box(data));
            }

            // Wait for processing
            std::thread::sleep(Duration::from_millis(500));
            let _ = running.wait();
        });
    });
}

fn benchmark_three_stage_throughput(c: &mut Criterion) {
    c.bench_function("three_stage_1000_msgs", |b| {
        b.iter(|| {
            let pipeline = PipelineBuilder::new()
                .add_stage("stage1", 1000, OverflowPolicy::Block)
                .add_stage("stage2", 1000, OverflowPolicy::Block)
                .add_stage("stage3", 1000, OverflowPolicy::Block)
                .build()
                .expect("Build failed");

            let input = pipeline.input();

            let running = pipeline
                .start(|_| Box::new(PassthroughStage))
                .expect("Start failed");

            for i in 0..1000 {
                let data = vec![i as u8; 64];
                let _ = input.push(black_box(data));
            }

            std::thread::sleep(Duration::from_millis(800));
            let _ = running.wait();
        });
    });
}

fn benchmark_high_throughput(c: &mut Criterion) {
    c.bench_function("high_throughput_5000_msgs", |b| {
        b.iter(|| {
            let pipeline = PipelineBuilder::new()
                .add_stage("stage1", 2000, OverflowPolicy::Block)
                .add_stage("stage2", 2000, OverflowPolicy::Block)
                .build()
                .expect("Build failed");

            let input = pipeline.input();

            let running = pipeline
                .start(|_| Box::new(PassthroughStage))
                .expect("Start failed");

            for i in 0..5000 {
                let data = vec![i as u8; 32];
                let _ = input.push(black_box(data));
            }

            std::thread::sleep(Duration::from_secs(1));
            let _ = running.wait();
        });
    });
}

criterion_group!(
    name = benches;
    config = Criterion::default().measurement_time(Duration::from_secs(10));
    targets = benchmark_single_stage_throughput, benchmark_three_stage_throughput, benchmark_high_throughput
);
criterion_main!(benches);
