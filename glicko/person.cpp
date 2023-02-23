#include "person.h"
#include <cmath>

Person::Person(uintf period, System *system, uintf n, float *times)
    : last_competed{period}, sigma2{system->start_sigma2},
      nu2{system->start_nu2}, system{system}, is_initialized{false} {
  initialize(n, times);
}

// Initialize mu, rho
void Person::initialize(uintf n, float *times) {

  // We cannot initialize anythin with no timed solves
  if (n == 0) {
    return;
  }
  // We can initialize mu to the only timed solve and use a default rho value
  if (n == 1) {
    mu = times[0];
    rho = system->start_rho;
  }
  // Intialize values with sample mean and standard deviation
  else {
    // Compute initial mu
    float sum = 0.0;
    for (uintf i = 0; i < n; ++i) {
      sum += times[i];
    }
    mu = sum / (float)n;

    // sum(x_i-xbar)^2
    float xix_sum = 0.0;
    for (uintf i = 0; i < n; ++i) {
      xix_sum += pow(times[i] - mu, 2);
    }
    // Sample standard deviation
    float stdev = sqrt(xix_sum / ((float)n - 1.0));
    // rho is the ratio of stdev to mean
    // (rho * mu)^2 is variance
    rho = stdev / mu;
  }
  is_initialized = true;
}

// Compute new rho
void Person::update_rho(float xbar, float elapsed2, uintf n, float *times) {

  // sum(xi-xbar)^2/(xbar^2)
  float frho_sum = 0.0;
  for (uintf i = 0; i < n; ++i) {
    frho_sum += pow(times[i] - xbar, 2);
  }
  frho_sum /= pow(xbar, 2);
  // f(rho)
  float frho = (float)n * pow(rho, 4) - frho_sum;
  // f'(rho)
  float dfrho = pow(rho, 3) / (system->nu2_const * nu2 * elapsed2) +
                3.0 * (float)n * pow(rho, 2);
  // Newton's method update
  rho -= frho / dfrho;
  rho = std::max(rho, system->min_rho);
}

// Compute new sigma^2
// Done after estimating new rho
void Person::update_sigma2(float xbar, float elapsed2, uintf n) {

  sigma2 =
      std::min(system->start_sigma2,
               std::max(system->min_sigma2,
                        1 / (1 / (system->sigma2_const * sigma2 * elapsed2) +
                             (float)n / pow(rho * xbar, 2))));
}

// Compute new mu
// Done after estimating new sigma^2
void Person::update_mu(float old_sigma2, float xsum, float xbar,
                       float elapsed2) {

  mu = sigma2 * (mu / (system->sigma2_const * old_sigma2 * elapsed2) +
                 xsum / pow(rho * xbar, 2));
}

// Compute new nu^2
// Done after estimating new mu
// Uses new mu instead of xbar
void Person::update_nu2(float elapsed2, uintf n, float *times) {

  // sum(xi-mi)^2
  float xmu_sum = 0.0;
  for (uintf i = 0; i < n; ++i) {
    xmu_sum += pow(times[i] - mu, 2);
  }
  nu2 = std::min(system->start_nu2,
                 std::max(system->min_nu2,
                          1 / (1 / (system->nu2_const * nu2 * elapsed2) +
                               3.0 * xmu_sum / (pow(rho, 4) * pow(mu, 2)))));
}

void update_stats(uintf period, uintf n, float *times) {

  if (n == 0) {
    return;
  }
  if (!is_initialized) {
    last_competed = period;
    initialize(n, times);
  } else {
    float elapsed2 = pow((float)(period - last_competed), 2);
    last_competed = period;
    float xsum = 0.0;
    for (uintf i = 0; i < n; ++i) {
      xsum += times[i];
    }
    float xbar = xsum / (float)n;

    update_rho(xbar, elapsed2, n, times);

    float old_sigma2 = sigma2;
    update_sigma2(xbar, elapsed2, n);

    update_mu(old_sigma2, xsum, xbar, elapsed2);

    update_nu2(elapsed2, n, times);
  }
}