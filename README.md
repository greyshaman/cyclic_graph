# Cyclic Graph

<center><img src="https://github.com/greyshaman/cyclic_graph/raw/refs/heads/main/logo/orig.webp" width="50%" alt="Rfs_tester Logo"></center>

[![Crates.io](https://img.shields.io/crates/v/cyclic_graph)](https://crates.io/crates/cyclic_graph)

A Rust library that implements a directed cyclic graph with one input and one output.

## Features

- Create Graph.
- Append or insert nodes into graph.
- Accept node by id.
- Remove node from graph with glue breaking links.
- Support asynchronously operation.

## Installation

Add the dependency to your `Cargo.toml`:

```toml
[dependencies]
cyclic_graph = "0.1.2"
```

## Overview

The idea of creating this library is to provide convenient placement of layers containing interacting neurons within a neuromorphic system. This system assumes a network with one input layer, many hidden layers, and one output layer.

In this network, signals are propagated and processed in a certain direction — from the input layer to the output layer through hidden ones. There may be cyclical connections between hidden layers, including the layer's connection to itself.

The system structure should be flexible and dynamically changeable. It should allow you to delete, add, and organize connections between layers, as well as provide access to layers and their payloads.

It is proposed to represent the layers as nodes of a graph and operate on them as with nodes of this graph.

## Managing nodes in a graph

In the initial state, the graph has only input and output nodes that are connected. These nodes cannot be removed or moved. The input node is always the starting point for other nodes in the graph, and the output is always an ending point for other nodes. Nodes can be added between already existing nodes. Connections are cut at the point of insertion and closed to the added node. Nodes can also be placed between existing nodes without altering existing connections. When a new node is inserted or added, its identifier is generated using a function specified during graph creation. If a node is removed (except for the input or output), connections must continue to the remaining nodes.

## Contributing

Contributions are welcome! If you'd like to contribute, please follow these steps:

1. Fork the repository.
2. Create a new branch for your feature or bugfix.
3. Make your changes and ensure all tests pass.
4. Submit a pull request with a detailed description of your changes.

## Licenses

This project is licensed under the MIT License.
