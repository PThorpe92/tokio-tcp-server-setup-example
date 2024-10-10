## Tokio TCP Client + Server Basic Example Repo

After a few times of spinning up a similar project layout and writing the
boilerplate to setup a server with a tcp connection with framed stream + sink
and some kind of shared `msg` type with a choice of serialization
proto, I figured I'd make a repo with this basic project structure all setup
to avoid having to do it all again myself.

It may be useful to someone else, because you never know.

### This example uses:

1. [Tokio](https://github.com/tokio-ts/tokio) async runtime, net::tcp connection, and channels/sync primitives

2. [Tokio tracing](https://github.com/tokio-rs/tracing) logging library

3. [rmp_serde](https://github.com/3Hren/msgpack-rust/tree/master/rmp-serde) msgpack for ser/deserialization


You can be easily replace `msgpack` either with something like `bincode` or `serde_json` or you can hand roll
your own binary serialization proto if you are feeling spunky.


A basic heartbeat implementation is included, but you'll notice everything is left very intentionally bare because 
you would likely end up ripping it out anyway if too much more was included here.


This is not meant to be some great example of exactly how to setup or use X or Y. How exactly you structure things like
your channels and ownership of different pieces is up to you. However if you think something can be improved, feel
free to open a PR, because why not, everything can be improved.
