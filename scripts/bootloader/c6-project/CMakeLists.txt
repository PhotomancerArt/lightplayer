# The smallest ESP-IDF project there is: it exists so `idf.py bootloader`
# has a project to build the second-stage bootloader subproject from. The app
# half is never built. See scripts/bootloader/build-c6-bootloader.sh.
cmake_minimum_required(VERSION 3.16)
include($ENV{IDF_PATH}/tools/cmake/project.cmake)
project(lp-c6-bootloader)
