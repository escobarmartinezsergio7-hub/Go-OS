#include <cstdlib>
#include <iostream>
#include <string>
#include <vector>

int main(int argc, char** argv) {
    std::cout << "ReduxOS newlib C++ sample\n";
    std::cout << "argc=" << argc << "\n";

    std::vector<std::string> args;
    for (int i = 0; i < argc; ++i) {
        args.emplace_back(argv[i] ? argv[i] : "");
    }

    for (size_t i = 0; i < args.size(); ++i) {
        std::cout << "argv[" << i << "] = " << args[i] << "\n";
    }

    std::string msg = "hello from static newlib c++";
    std::cout << msg << "\n";
    return EXIT_SUCCESS;
}
