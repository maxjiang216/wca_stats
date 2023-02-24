#include "person.h"
#include <cmath>
#include <iostream>

Person::Person(System *system)
    : mu{0}, sigma2{system->start_sigma2}, rho{system->start_rho},
      nu2{system->start_nu2}, system{system}, is_initialized{false} {}

// Initialize mu, rho
void Person::initialize(uintf n, double *times) {

  // We cannot initialize anythin with no timed solves
  if (n == 0) {
    return;
  }
  // We can initialize mu to the only timed solve and use a default rho value
  if (n == 1) {
    mu = times[0];
    rho = system->start_rho;
    update_sigma2(mu, sigma2, n);
  }
  // Intialize values with sample mean and standard deviation
  else {
    // Compute initial mu
    double sum = 0.0;
#pragma omp simd
    for (uintf i = 0; i < n; ++i) {
      sum += times[i];
    }
    mu = sum / (double)n;

    // sum(x_i-xbar)^2
    double xix_sum = 0.0;
#pragma omp simd
    for (uintf i = 0; i < n; ++i) {
      double diff = times[i] - mu;
      xix_sum += diff * diff;
    }
    // Sample standard deviation
    double stdev = sqrt(xix_sum / ((double)n - 1.0));
    // rho is the ratio of stdev to mean
    // (rho * mu)^2 is variance
    rho = stdev / mu;
    update_sigma2(mu, sigma2, n);
    update_nu2(nu2, n, times);
  }
  is_initialized = true;
}

// Compute new rho
void Person::update_rho(double xbar, double cur_nu2, uintf n, double *times) {

  // sum(xi-xbar)^2/(xbar^2)
  double frho_sum = 0.0;
#pragma omp simd
  for (uintf i = 0; i < n; ++i) {
    double diff = times[i] - xbar;
    frho_sum += diff * diff;
  }
  frho_sum /= pow(xbar, 2);
  // f(rho)
  double frho = (double)n * pow(rho, 4) - frho_sum;
  // f'(rho)
  double dfrho = pow(rho, 3) / cur_nu2 + 3.0 * (double)n * pow(rho, 2);
  // Newton's method update
  rho -= frho / dfrho;
  rho = std::max(rho, system->min_rho);
}

// Compute new sigma^2
// Done after estimating new rho
void Person::update_sigma2(double xbar, double cur_sigma2, uintf n) {

  sigma2 =
      std::min(system->start_sigma2,
               std::max(system->min_sigma2,
                        1.0 / (1.0 / cur_sigma2 + (double)n / pow(rho * xbar, 2))));
}

// Compute new mu
// Done after estimating new sigma^2
void Person::update_mu(double cur_sigma2, double xsum, double xbar, uintf n) {

  mu = (1.0 / (1.0 / cur_sigma2 + (double)n / pow(rho * xbar, 2))) * (mu / cur_sigma2 + xsum / pow(rho * xbar, 2));
}

// Compute new nu^2
// Done after estimating new mu
// Uses new mu instead of xbar
void Person::update_nu2(double cur_nu2, uintf n, double *times) {

  // sum(xi-mi)^2
  double xmu_sum = 0.0;
#pragma omp simd
  for (uintf i = 0; i < n; ++i) {
    double diff = times[i] - mu;
    xmu_sum += diff * diff;
  }
  nu2 = std::min(system->start_nu2,
                 std::max(system->min_nu2,
                          1.0 / (1.0 / cur_nu2 +
                                 3.0 * xmu_sum / (pow(rho, 4) * pow(mu, 2)))));
}

void Person::update_stats(uintf period, uintf n, double *times) {

  if (n == 0) {
    return;
  }
  if (!is_initialized) {
    last_competed = period;
    initialize(n, times);
  } else {
    double elapsed = (double)(period - last_competed);
    // Vairance governing prior distribution of mu and rho
    double cur_sigma2 =
        std::min(system->sigma2_const * sigma2 * elapsed, system->start_sigma2);
    double cur_nu2 =
        std::min(system->nu2_const * nu2 * elapsed, system->start_nu2);
    last_competed = period;
    double xsum = 0.0;
#pragma omp simd
    for (uintf i = 0; i < n; ++i) {
      xsum += times[i];
    }
    double xbar = xsum / (double)n;

    update_rho(xbar, cur_nu2, n, times);

    update_sigma2(xbar, cur_sigma2, n);

    update_mu(cur_sigma2, xsum, xbar, n);

    update_nu2(cur_nu2, n, times);
  }
}

double Person::get_mu() const { return mu; }