# Tracing experiments

The purpose of this repository is to show how we might go about porting our logging infrastructure
to the tracing crate, before taking the leap into using OpenTelemetry.

It's really here to demo that if we change the instrumentation of our codebases to use tracing, we
can use that same instrumentation to introduce OpenTelemetry at a later date.

## What's in the box?

`layer` is a custom implementation of the `Layer` trait from tracing-subscriber. It allows us to
emit logs in the same format as we are currently and to access fields in the tree of parent spans in
order to access things like "correlation-id" so that we can continue with our home-rolled
distributed tracing.

`demo` uses `layer` to emit logs to stdout, and if started in the accompanying docker-compose also
emits span data to jaeger.

## Run it

If you're using Colima then you're going to have to make use of the docker-compose in the root of
the repository as you can't bind UDP ports to the host without a lot of workarounds. This will also
avoid the issue of having an MSRV of 1.70 for `demo`.

The following command should get you going.

```sh
docker-compose up --build
```

Then navigate to [here](http://localhost:8080) for the ascii cats, and [here](http://localhost:16686) for
jaeger.

## Credits

* I've read (and lifted) some of the code from tracing-subscriber and tracing-bunyan-formatter which
  I used as a reference for the implementation.
* I stole the idea for a "catscii" API from fasterthanlime's blog as I didn't have the energy to be
  creative and it's got just enough in there to be useful for demoing tracing.

## TODO

- [ ] Document the implementation of `Layer` better so that it can be used as a teaching device.
- [ ] Try implementing `FormatEvent` instead of a whole `Layer` to reduce surface area of the code
  that we will need to maintain (albeit temporarily).
- [ ] Try using OTEL collector instead of sending UDP packets directly to Jaeger.
