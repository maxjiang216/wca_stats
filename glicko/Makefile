CXX = g++
CXXFLAGS = -std=c++20 -Wall -MMD -O3 -fopenmp -g -fopt-info-vec-optimized-missed #-pg

SRC = .

SRCS = $(shell find $(SRC) -name '*.cpp')

OBJS = ${SRCS:.cpp=.o}

DPDS = ${OBJS:.o=.d}

all: rate
rate: $(CXXFLGS) $(OBJS)
	$(CXX)  $(OBJS) $(CXXFLGS) -O3 -fopenmp -g -fopt-info-vec-optimized-missed -o rate

-include ${DPDS}

.PHONY: clean

clean:
	rm ${OBJS} ${DPDS}