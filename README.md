# block-alloc-appendbuf

> A Sync append-only buffer with Send views that uses block_allocator::Allocator. 

This has been adapted directly from [appendbuf](https://github.com/reem/appendbuf) by [reem](https://github.com/reem) 
It has been updated to use a fixed-size allocator, which means the API has changed to take a ptr to the block_allocator
instead of a size, when creating new buffers. 

## [Documentation](https://crates.fyi/crates/appendbuf/0.1.6)

Provides an atomically reference counted, append-only buffer. Each buffer
consists of a unique `AppendBuf` handle which can write new data to the buffer
and any number of atomically reference counted `Slice` handles, which contain
read-only windows into data previously written to the buffer.

## Example

```rust
extern crate appendbuf;

use appendbuf::AppendBuf;

fn main() {
    // Create an AppendBuf with capacity for 100 bytes.
    let mut buf = AppendBuf::new(100);

    // Write some data in pieces.
    assert_eq!(buf.fill(&[1, 2, 3, 4]), 4);
    assert_eq!(buf.fill(&[10, 12, 13, 14, 15]), 5);
    assert_eq!(buf.fill(&[34, 35]), 2);

    // Read all the data we just wrote.
    assert_eq!(&*buf.slice(), &[1, 2, 3, 4, 10, 12, 13, 14, 15, 34, 35]);
}
```

## Usage

Use the crates.io repository; add this to your `Cargo.toml` along
with the rest of your dependencies:

```toml
[dependencies]
appendbuf = "0.1"
```

## Author

[Jonathan Reem](https://medium.com/@jreem) is the primary author and maintainer of appendbuf.

## License

MIT

