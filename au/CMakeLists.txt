# Builds Jovian-8.component (Audio Unit v2 instrument for Logic) with clap-wrapper around the
# Rust CLAP. The Rust static library (exporting `clap_entry`) is linked into the component, so
# the bundle is self-contained.
#
#   cmake -S au -B build/au -G Ninja -DJOVIAN_STATICLIB=$PWD/target/release/libjovian_plugin.a
#   cmake --build build/au
cmake_minimum_required(VERSION 3.21)
set(CMAKE_OSX_DEPLOYMENT_TARGET 11.0 CACHE STRING "")
set(CMAKE_OSX_ARCHITECTURES arm64 CACHE STRING "")
project(jovian8_au VERSION 2.0.0 LANGUAGES C CXX OBJC OBJCXX)

set(CMAKE_CXX_STANDARD 17)
set(CLAP_WRAPPER_DOWNLOAD_DEPENDENCIES TRUE CACHE BOOL "" FORCE)
set(CLAP_WRAPPER_BUILD_AUV2 TRUE CACHE BOOL "" FORCE)
set(CLAP_WRAPPER_DONT_ADD_TARGETS TRUE CACHE BOOL "" FORCE)
add_subdirectory(${CMAKE_CURRENT_SOURCE_DIR}/../third_party/clap-wrapper ${CMAKE_BINARY_DIR}/clap-wrapper)

if (NOT JOVIAN_STATICLIB)
    message(FATAL_ERROR "Set JOVIAN_STATICLIB to target/release/libjovian_plugin.a")
endif()

add_library(jovian8_auv2 MODULE entry.cpp)
target_link_options(jovian8_auv2 PRIVATE "-Wl,-force_load,${JOVIAN_STATICLIB}")
target_link_libraries(jovian8_auv2 PRIVATE
        "${JOVIAN_STATICLIB}"
        objc iconv
        "-framework CoreFoundation" "-framework Foundation" "-framework AppKit"
        "-framework CoreGraphics" "-framework QuartzCore" "-framework CoreText"
        "-framework CoreVideo" "-framework IOKit" "-framework Carbon" "-framework Security")
set_property(TARGET jovian8_auv2 APPEND PROPERTY LINK_DEPENDS "${JOVIAN_STATICLIB}")

target_add_auv2_wrapper(
        TARGET jovian8_auv2
        OUTPUT_NAME "Jovian-8"
        BUNDLE_IDENTIFIER "com.anomatver.jovian8"
        BUNDLE_VERSION "${PROJECT_VERSION}"
        MANUFACTURER_NAME "Jovian"
        MANUFACTURER_CODE "Jovn"
        SUBTYPE_CODE "JV8e"
        INSTRUMENT_TYPE "aumu"
)
