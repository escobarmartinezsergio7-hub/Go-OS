#----------------------------------------------------------------
# Generated CMake target import file.
#----------------------------------------------------------------

# Commands may need to know the format version.
set(CMAKE_IMPORT_FILE_VERSION 1)

# Import target "litehtml" for configuration ""
set_property(TARGET litehtml APPEND PROPERTY IMPORTED_CONFIGURATIONS NOCONFIG)
set_target_properties(litehtml PROPERTIES
  IMPORTED_LINK_INTERFACE_LANGUAGES_NOCONFIG "CXX"
  IMPORTED_LOCATION_NOCONFIG "${_IMPORT_PREFIX}/lib/liblitehtml.a"
  )

list(APPEND _cmake_import_check_targets litehtml )
list(APPEND _cmake_import_check_files_for_litehtml "${_IMPORT_PREFIX}/lib/liblitehtml.a" )

# Commands beyond this point should not need to know the version.
set(CMAKE_IMPORT_FILE_VERSION)
