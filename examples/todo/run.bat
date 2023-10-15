@echo off
cd ..\..
cargo build
cd examples\todo
..\..\target\debug\webx.exe run -l 4
