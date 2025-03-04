use ray_tracer::RayTracer;

fn main() {
    let mut tracer = RayTracer::default();
    tracer.run().unwrap();
}
