cmake_minimum_required(VERSION 3.15)
project(node-key-secure C)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

add_executable(node-key-secure
    main.c
    atecc608.c
)

target_include_directories(node-key-secure PRIVATE ${CMAKE_CURRENT_SOURCE_DIR})
