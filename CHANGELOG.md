# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)

## [0.2.0] - 2025-06-22

### Changed

- Up crate minor version
- Using content dyn trait object for layer type content
- Optimized node structure: use content enum to implement simple and layer types cases
- Moved content types (Simple and Layer) into separate module.

### Added

- Added dependencies from async-trait, tokio-util, futures
- Added new error variants
- Added links_acceptor and links_provider  traits for using inner connection establishing handlers
- Added handlers usage in node entity
- Added content enum with Simple and Layer variants
- Added error type on short link node to itself

## [0.1.2] - 2025-04-09

### Changed

- improved code and fixed some code style issues
- node stored payload in content instead of data struct member

## [0.1.1] - 2025-04-07

### Added

- Added default constructor CyclicGraph::new_default with default id_generator for usize id or String id.

### Changed

- moved Generator_mode to id_generator module

## [0.1.0] - 2025-04-07

### Added

- Started project
- Added Node structure with unit tests
- Added Error types
- Added CyclicGraph structure with unit tests
- Added logo image
- Added README.md
- Added CHANGELOG.md
